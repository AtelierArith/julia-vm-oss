//! Thread-local compilation cache for Base functions
//!
//! This module provides a thread-local cache for precompiled Base functions
//! to dramatically reduce compilation time.
//!
//! Strategy:
//! - Each thread has its own cache (thread_local!)
//! - Base functions (~460 functions) are compiled once per thread
//! - Subsequent compilations reuse the cached result
//!
//! This approach avoids the need to make CompiledProgram thread-safe (Send/Sync)
//! while still providing excellent performance for benchmarks and single-threaded use.

use super::types::CResult;
use crate::ir::core::Program;
use crate::vm::{CompiledProgram, StructDefInfo, ValueType};
use std::cell::RefCell;
use std::collections::hash_map::DefaultHasher;
use std::collections::HashMap;
use std::env;
use std::hash::{Hash, Hasher};
use std::rc::Rc;

/// Check if cache debug logging is enabled via environment variable
fn should_log_cache() -> bool {
    env::var("SUBSET_JULIA_VM_CACHE_DEBUG").is_ok()
}

/// Check if cache is disabled via environment variable
fn is_cache_disabled() -> bool {
    env::var("SUBSET_JULIA_VM_DISABLE_CACHE").is_ok()
}

/// Return true when Base cache must be bypassed for correctness.
///
/// Base cache is indexed against the full prelude function order. When a program's
/// `base_function_count` is smaller than that prelude count (because user code replaced
/// one or more Base methods with exact-signature overrides), cached indices no longer
/// align with the merged function list.
#[inline]
fn should_skip_base_cache_for_program(program: &Program, prelude_function_count: usize) -> bool {
    program.base_function_count > 0 && program.base_function_count != prelude_function_count
}

/// Log cache message only if debug logging is enabled
#[inline]
fn log_cache(msg: &str) {
    if should_log_cache() {
        use std::io::Write;
        let _ = writeln!(std::io::stderr(), "{msg}");
    }
}

/// Cached compilation data including both bytecode and method tables.
/// Wrapped in `Rc` for zero-cost sharing from thread-local cache (Issue #3357).
struct CachedBase {
    compiled: CompiledProgram,
    method_tables: HashMap<String, super::MethodTable>,
    /// Closure captures from Base function compilation (Issue #2100).
    /// Needed so inner functions from Base can correctly emit LoadCaptured instructions.
    closure_captures: HashMap<String, std::collections::HashSet<String>>,
    /// Promotion rules extracted during Base compilation (Issue #3036).
    /// Stored here so get_or_init_base_cache() can replay them into the thread-local
    /// promotion registry when the registry has been cleared (e.g., between test runs).
    promotion_rules: Vec<(String, String, String)>,
}

thread_local! {
    /// Thread-local cache for Base compilation (bytecode + method tables).
    /// Wrapped in `Rc` to avoid deep-cloning on every retrieval (Issue #3357).
    static BASE_CACHE: RefCell<Option<Rc<CachedBase>>> = const { RefCell::new(None) };

    /// Thread-local cache for full programs (keyed by program hash)
    /// Useful for benchmarks and repeated compilations of identical code
    static PROGRAM_CACHE: RefCell<HashMap<u64, CompiledProgram>> = RefCell::new(HashMap::new());
}

/// Compile only the Base functions from the prelude with method tables
fn compile_base_functions() -> CResult<CachedBase> {
    // Try embedded cache first (build-time precompiled, Issue #2929)
    if let Some(embedded) = super::embedded_cache::load_embedded_cache() {
        log_cache("[Base Cache] Using embedded precompiled Base cache");

        // Replay promotion rules from the embedded cache into the thread-local registry
        for (t1, t2, ret) in &embedded.promotion_rules {
            super::promotion::register_promotion_rule(t1, t2, ret);
        }
        super::promotion::mark_registry_initialized();

        return Ok(CachedBase {
            compiled: embedded.compiled,
            method_tables: embedded.method_tables,
            closure_captures: embedded.closure_captures,
            // Store rules in CachedBase so get_or_init_base_cache can replay on cache hits (Issue #3036)
            promotion_rules: embedded.promotion_rules,
        });
    }

    // Fallback: compile at runtime
    // Get the prelude program (already parsed and lowered)
    let prelude = match crate::get_prelude_program() {
        Some(p) => p,
        None => return super::types::err("Prelude not available"),
    };

    // Create a Base-only program with prelude main block
    // IMPORTANT: Include prelude main block to capture const definitions (e.g., pathsep_char)
    // IMPORTANT: Include modules from prelude to capture Meta module functions
    let base_program = Program {
        functions: prelude.functions.clone(),
        structs: prelude.structs.clone(),
        abstract_types: prelude.abstract_types.clone(),
        type_aliases: prelude.type_aliases.clone(),
        main: prelude.main.clone(),
        modules: prelude.modules.clone(),
        usings: vec![],
        macros: vec![],
        enums: vec![],
        base_function_count: prelude.functions.len(),
    };

    // Compile Base functions and capture method tables + closure captures
    let (compiled, method_tables, closure_captures) = super::compile_core_program_internal(
        &base_program,
        &std::collections::HashMap::new(),
        &std::collections::HashMap::new(),
        super::CompilerCacheInput::default(),
    )?;

    log_cache(&format!(
        "[Base Cache] Compiled {} Base functions, {} instructions, {} method tables, {} closure captures",
        compiled.functions.len(),
        compiled.code.len(),
        method_tables.len(),
        closure_captures.len()
    ));

    // Extract promotion rules directly from the prelude function IR bodies (Issue #3025).
    // The method-table approach (extract_promotion_rules) is broken because the type
    // inference engine infers all promote_rule return types as ValueType::Any.
    // Reading from the IR body avoids inference and correctly captures both primitive
    // (e.g., Int64) and struct (e.g., Complex{Float64}, Rational{Int64}) return types.
    extract_promotion_rules_from_ir(&base_program.functions);

    // Store promotion rules in CachedBase so get_or_init_base_cache can replay them
    // when the thread-local registry is cleared (e.g., between test runs). (Issue #3036)
    let promotion_rules = super::promotion::get_all_promotion_rules();

    Ok(CachedBase {
        compiled,
        method_tables,
        closure_captures,
        promotion_rules,
    })
}

/// Extract the return type name from a `promote_rule` function body.
///
/// The type inference engine infers `promote_rule` return types as `ValueType::Any`
/// because functions return type objects (e.g., `Int64` in return position), not values.
/// This function bypasses inference by reading the type name directly from the IR body.
///
/// Two body patterns are recognised:
/// - Primitive type: `Stmt::Expr { expr: Var("Int64") }` → `"Int64"`
/// - Struct type: `Stmt::Expr { expr: Builtin { TypeOf, [Literal::Str("Complex{Float64}")] } }` → `"Complex{Float64}"`
fn extract_return_type_from_promote_rule_body(
    body: &crate::ir::core::Block,
) -> Option<String> {
    use crate::ir::core::{BuiltinOp, Expr, Literal, Stmt};

    // promote_rule bodies are a single expression statement
    if body.stmts.len() != 1 {
        return None;
    }

    match &body.stmts[0] {
        Stmt::Expr { expr, .. } => match expr {
            // Primitive type return: `Int64`, `Float64`, etc.
            Expr::Var(name, _) => Some(name.clone()),
            // Struct type return: `Builtin { TypeOf, [Literal(Str("Complex{Float64}"))] }`
            // This is how parametric struct type objects are represented in the IR.
            Expr::Builtin {
                name: BuiltinOp::TypeOf,
                args,
                ..
            } => {
                if let [Expr::Literal(Literal::Str(type_name), _)] = args.as_slice() {
                    Some(type_name.clone())
                } else {
                    None
                }
            }
            _ => None,
        },
        _ => None,
    }
}

/// Check whether a `Type{T}` parameter contains a type variable.
/// Skips generic promote_rule definitions like `promote_rule(::Type{T}, ::Type{S})`.
fn is_typeof_with_type_var(ty: &crate::types::JuliaType) -> bool {
    use crate::types::JuliaType;
    matches!(ty, JuliaType::TypeOf(inner) if matches!(inner.as_ref(), JuliaType::TypeVar(_, _)))
}

/// Extract promotion rules directly from prelude function IR bodies and register them.
///
/// This replaces the method-table approach (`extract_promotion_rules`) which was broken
/// because the type inference engine infers all `promote_rule` return types as
/// `ValueType::Any` (Issue #3025). Reading the return type directly from the function
/// body avoids the inference step entirely.
///
/// Registration is done into the thread-local promotion rule registry.
fn extract_promotion_rules_from_ir(functions: &[crate::ir::core::Function]) {
    let mut count = 0;
    for func in functions {
        if func.name != "promote_rule" || func.params.len() != 2 {
            continue;
        }

        let p0_type = func.params[0].effective_type();
        let p1_type = func.params[1].effective_type();

        // Skip generic promote_rule(::Type{T}, ::Type{S}) — these return Union{} / Bottom
        if is_typeof_with_type_var(&p0_type) || is_typeof_with_type_var(&p1_type) {
            continue;
        }

        let type1 = extract_type_from_typeof(&p0_type);
        let type2 = extract_type_from_typeof(&p1_type);
        let return_type = extract_return_type_from_promote_rule_body(&func.body);

        if let (Some(t1), Some(t2), Some(ret)) = (type1, type2, return_type) {
            super::promotion::register_promotion_rule(&t1, &t2, &ret);
            count += 1;
            log_cache(&format!(
                "[Promotion] Registered: promote_rule({}, {}) = {}",
                t1, t2, ret
            ));
        }
    }
    log_cache(&format!(
        "[Promotion] Registered {} promotion rules from Julia IR (Issue #3025)",
        count
    ));

    // Mark registry as initialized
    super::promotion::mark_registry_initialized();
}

/// Extract the type name from a Type{T} parameter.
/// e.g., TypeOf(Int64) -> Some("Int64"), TypeOf(Complex{Float64}) -> Some("Complex{Float64}")
pub(super) fn extract_type_from_typeof(ty: &crate::types::JuliaType) -> Option<String> {
    use crate::types::JuliaType;

    match ty {
        JuliaType::TypeOf(inner) => {
            // The inner type is the actual type being passed
            Some(inner.name().to_string())
        }
        // Also handle DataType for some cases
        JuliaType::DataType => None, // Generic DataType, can't extract specific type
        _ => None,
    }
}

/// Extract the return type name from a ValueType.
///
/// `struct_defs` maps type IDs to struct definitions, enabling resolution of
/// `ValueType::Struct(id)` to concrete names like "Complex{Float64}" or "Rational{Int64}".
///
/// Not used in the primary promotion-rule extraction path (which now reads directly from
/// IR function bodies via `extract_promotion_rules_from_ir`). Kept for tests and potential
/// future use.
#[allow(dead_code)]
pub(super) fn extract_return_type_name(
    vt: &crate::vm::ValueType,
    struct_defs: &[StructDefInfo],
) -> Option<String> {
    use crate::vm::ValueType;

    match vt {
        // DataType returns indicate a generic type object was returned.
        // The compiler infers DataType when the specific type can't be determined
        // statically (e.g., conditional returns of different types).
        // These are handled by the Rust fallback in promotion.rs.
        ValueType::DataType => None,
        // For concrete types, we can determine the name
        ValueType::I64 => Some("Int64".to_string()),
        ValueType::I32 => Some("Int32".to_string()),
        ValueType::I16 => Some("Int16".to_string()),
        ValueType::I8 => Some("Int8".to_string()),
        ValueType::I128 => Some("Int128".to_string()),
        ValueType::F64 => Some("Float64".to_string()),
        ValueType::F32 => Some("Float32".to_string()),
        ValueType::Bool => Some("Bool".to_string()),
        ValueType::Str => Some("String".to_string()),
        // Struct types - look up the name from struct_defs using the type_id
        ValueType::Struct(id) => struct_defs.get(*id).map(|def| def.name.clone()),
        // Nothing type (Union{})
        ValueType::Nothing => Some("Union{}".to_string()),
        _ => None,
    }
}

/// Get or initialize the Base cache for this thread.
///
/// Returns an `Rc<CachedBase>` — callers share the cached data via
/// reference counting instead of deep-cloning (Issue #3357).
fn get_or_init_base_cache() -> CResult<Rc<CachedBase>> {
    BASE_CACHE.with(|cache| {
        let mut cache = cache.borrow_mut();
        if cache.is_none() {
            *cache = Some(Rc::new(compile_base_functions()?));
        }
        let cached = Rc::clone(
            cache
                .as_ref()
                .ok_or_else(|| super::types::CompileError::Msg("Base cache unavailable".to_string()))?,
        );

        // Replay promotion rules if the thread-local registry was cleared after the initial
        // Base compilation (e.g., between test runs). This ensures the invariant:
        // after compile_with_cache(), is_registry_initialized() == true. (Issue #3036)
        if !super::promotion::is_registry_initialized() {
            for (t1, t2, ret) in &cached.promotion_rules {
                super::promotion::register_promotion_rule(t1, t2, ret);
            }
            super::promotion::mark_registry_initialized();
        }

        Ok(cached)
    })
}

/// Check if Base cache is initialized for this thread
pub fn is_cache_initialized() -> bool {
    BASE_CACHE.with(|cache| cache.borrow().is_some())
}

/// Clear the Base cache and all associated thread-local registries (mainly for testing).
///
/// Clears both BASE_CACHE and all registries that are populated during Base
/// compilation, ensuring consistent state. Registries cleared:
/// - `PROMOTION_RULE_REGISTRY` (promotion rules extracted from Julia definitions)
///
/// Note: show_methods is embedded in `CompiledProgram` inside `CachedBase`, so
/// it is cleared automatically when BASE_CACHE is cleared.
///
/// Invariant: after `clear_cache()`, all associated registries are also cleared.
/// This prevents the desync bug where BASE_CACHE is populated but registries
/// are empty (Issue #3038, #3036).
pub fn clear_cache() {
    BASE_CACHE.with(|cache| *cache.borrow_mut() = None);
    PROGRAM_CACHE.with(|cache| cache.borrow_mut().clear());
    // Clear the promotion registry together with BASE_CACHE to maintain
    // the invariant that registries and cache are always in sync (Issue #3038).
    super::promotion::clear_registry();
}

/// Export the current Base cache for serialization (used by --precompile-base).
/// Returns None if cache is not initialized.
pub(crate) fn export_base_cache() -> Option<(
    CompiledProgram,
    HashMap<String, super::MethodTable>,
    HashMap<String, std::collections::HashSet<String>>,
)> {
    BASE_CACHE.with(|cache| {
        cache.borrow().as_ref().map(|c| {
            (
                c.compiled.clone(),
                c.method_tables.clone(),
                c.closure_captures.clone(),
            )
        })
    })
}

/// Compute a hash of the program for caching
/// This creates a fingerprint based on the program structure
fn compute_program_hash(
    program: &Program,
    global_types: &HashMap<String, ValueType>,
    global_struct_names: &HashMap<String, String>,
) -> u64 {
    let mut hasher = DefaultHasher::new();

    // Hash main block (the actual user code)
    format!("{:?}", program.main).hash(&mut hasher);

    // Hash user function count (base functions are always the same)
    let user_func_count = program.functions.len() - program.base_function_count;
    user_func_count.hash(&mut hasher);

    // Hash user functions (skip base functions as they're constant)
    for func in program.functions.iter().skip(program.base_function_count) {
        format!("{:?}", func).hash(&mut hasher);
    }

    // Hash user structs
    for s in &program.structs {
        format!("{:?}", s).hash(&mut hasher);
    }

    // Hash modules
    for m in &program.modules {
        format!("{:?}", m).hash(&mut hasher);
    }

    // Hash global type context in deterministic key order
    let mut global_type_entries: Vec<_> = global_types.iter().collect();
    global_type_entries.sort_by(|(a, _), (b, _)| a.cmp(b));
    for (name, ty) in global_type_entries {
        name.hash(&mut hasher);
        format!("{:?}", ty).hash(&mut hasher);
    }

    // Hash struct-name context used to resolve stable struct IDs in REPL
    let mut global_struct_entries: Vec<_> = global_struct_names.iter().collect();
    global_struct_entries.sort_by(|(a, _), (b, _)| a.cmp(b));
    for (name, struct_name) in global_struct_entries {
        name.hash(&mut hasher);
        struct_name.hash(&mut hasher);
    }

    hasher.finish()
}

// NOTE: Full incremental cache optimization is deferred due to compiler architecture constraints.
//
// The original plan was to:
// 1. Compile Base functions once and cache
// 2. Compile only user functions (skipping Base)
// 3. Merge cached Base bytecode + user bytecode
//
// However, this requires significant compiler refactoring because:
// - The compiler needs to see all functions to resolve references
// - Compiling functions in isolation fails (e.g., "Unknown function: gcd")
// - Extracting only user bytecode from full compilation doesn't save time
//
// Current approach: We cache entire compiled Base output and reuse it via
// `CompilerCacheInput::precompiled_base`. The embedded precompiled cache
// (Issue #2929) also provides a build-time cache for fast startup.

/// Compile a program using multi-level caching for maximum speedup
///
/// Strategy:
/// 1. Check full program cache (Option C) - if identical program, return immediately
/// 2. Get or initialize cached Base functions (Option A - partial)
/// 3. Compile with precompiled Base, reusing cached bytecode
///
/// Speedup levels:
/// - Full program cache hit: ~99% speedup (0.05ms vs 4ms)
/// - Base cache hit: ~65% speedup (1.4ms vs 4ms)
/// - No cache: baseline (4ms)
pub fn compile_with_cache(program: &Program) -> CResult<CompiledProgram> {
    compile_with_cache_with_globals(
        program,
        &std::collections::HashMap::new(),
        &std::collections::HashMap::new(),
    )
}

/// Compile a program using cache with REPL/global type context.
pub fn compile_with_cache_with_globals(
    program: &Program,
    global_types: &HashMap<String, ValueType>,
    global_struct_names: &HashMap<String, String>,
) -> CResult<CompiledProgram> {
    // If cache is disabled, use regular compilation (for development/testing)
    if is_cache_disabled() {
        log_cache("[Cache] DISABLED via SUBSET_JULIA_VM_DISABLE_CACHE");
        return super::compile_core_program_with_globals(program, global_types, global_struct_names);
    }

    // Option C: Check full program cache first
    let program_hash = compute_program_hash(program, global_types, global_struct_names);
    let cached_program = PROGRAM_CACHE.with(|cache| cache.borrow().get(&program_hash).cloned());

    if let Some(compiled) = cached_program {
        log_cache("[Cache] FULL HIT - reusing entire compiled program");
        return Ok(compiled);
    }

    // Issue #2726 / #2790: Base cache requires exact function-order alignment.
    // If prelude methods were replaced by user exact-signature definitions,
    // `base_function_count` shrinks and cached indices become invalid.
    let prelude_function_count = crate::get_prelude_program()
        .map(|p| p.functions.len())
        .unwrap_or(program.base_function_count);
    if should_skip_base_cache_for_program(program, prelude_function_count) {
        log_cache(
            "[Cache] Base cache BYPASS - base_function_count differs from prelude count; using full compile path",
        );
        let compiled = super::compile_core_program_with_globals(program, global_types, global_struct_names)?;
        PROGRAM_CACHE.with(|cache| {
            cache.borrow_mut().insert(program_hash, compiled.clone());
        });
        return Ok(compiled);
    }

    // Cache miss - proceed with Base caching (Option A + Base bytecode caching)
    let cache_was_initialized = is_cache_initialized();
    let base_cache = get_or_init_base_cache()?;

    if !cache_was_initialized {
        log_cache(&format!(
            "[Cache] Compiled {} Base functions + {} method tables (first time)",
            base_cache.compiled.functions.len(),
            base_cache.method_tables.len()
        ));
    } else {
        log_cache(&format!(
            "[Cache] Base HIT - reusing {} cached Base functions + {} method tables",
            base_cache.compiled.functions.len(),
            base_cache.method_tables.len()
        ));
    }

    // Compile with precompiled Base bytecode AND cached method tables + closure captures (Option A!)
    let (compiled, _final_method_tables, _final_closure_captures) =
        super::compile_core_program_internal(
            program,
            global_types,
            global_struct_names,
            super::CompilerCacheInput {
                precompiled_base: Some(&base_cache.compiled),
                method_tables: Some(&base_cache.method_tables),
                closure_captures: Some(&base_cache.closure_captures),
            },
        )?;

    log_cache(&format!(
        "[Cache] Compiled {} user functions + main",
        program.functions.len() - program.base_function_count
    ));

    // Store in full program cache for future use
    PROGRAM_CACHE.with(|cache| {
        cache.borrow_mut().insert(program_hash, compiled.clone());
    });

    Ok(compiled)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::compile::promotion;
    use crate::vm::{StructDefInfo, ValueType};

    fn parse_and_lower_ok(src: &str) -> Program {
        crate::pipeline::parse_and_lower(src).expect("pipeline error")
    }

    #[test]
    fn test_should_skip_base_cache_when_user_replaces_base_signature() {
        let program = parse_and_lower_ok("identity(x) = x");
        let prelude = crate::get_prelude_program().expect("prelude must be loaded");
        assert!(program.base_function_count < prelude.functions.len());
        assert!(should_skip_base_cache_for_program(&program, prelude.functions.len()));
    }

    #[test]
    fn test_should_not_skip_base_cache_when_base_count_matches_prelude() {
        let program = parse_and_lower_ok("x = 1");
        let prelude = crate::get_prelude_program().expect("prelude must be loaded");
        assert_eq!(program.base_function_count, prelude.functions.len());
        assert!(!should_skip_base_cache_for_program(
            &program,
            prelude.functions.len()
        ));
    }

    /// Regression test for Issue #2908: extract_return_type_name must resolve
    /// ValueType::Struct(id) to the struct name using struct_defs.
    /// Before the fix, it returned None for all Struct types, silently dropping
    /// Complex and Rational promotion rules from the cache.
    #[test]
    fn test_extract_return_type_name_with_struct_types() {
        let struct_defs = vec![
            StructDefInfo {
                name: "Complex{Float64}".to_string(),
                is_mutable: false,
                fields: vec![
                    ("re".to_string(), ValueType::F64),
                    ("im".to_string(), ValueType::F64),
                ],
                parent_type: None,
            },
            StructDefInfo {
                name: "Rational{Int64}".to_string(),
                is_mutable: false,
                fields: vec![
                    ("num".to_string(), ValueType::I64),
                    ("den".to_string(), ValueType::I64),
                ],
                parent_type: None,
            },
        ];

        // Struct(0) must resolve to the first struct's name
        assert_eq!(
            extract_return_type_name(&ValueType::Struct(0), &struct_defs),
            Some("Complex{Float64}".to_string()),
            "Struct(0) should resolve to 'Complex{{Float64}}'"
        );

        // Struct(1) must resolve to the second struct's name
        assert_eq!(
            extract_return_type_name(&ValueType::Struct(1), &struct_defs),
            Some("Rational{Int64}".to_string()),
            "Struct(1) should resolve to 'Rational{{Int64}}'"
        );

        // Out-of-bounds index must return None (not panic)
        assert_eq!(
            extract_return_type_name(&ValueType::Struct(99), &struct_defs),
            None,
            "Out-of-bounds Struct index should return None"
        );
    }

    /// extract_return_type_name must return None for Struct(id) when struct_defs is empty.
    #[test]
    fn test_extract_return_type_name_struct_with_empty_defs() {
        let struct_defs: Vec<StructDefInfo> = vec![];
        assert_eq!(
            extract_return_type_name(&ValueType::Struct(0), &struct_defs),
            None,
            "Struct(0) with empty struct_defs should return None"
        );
    }

    /// extract_return_type_name must handle primitive ValueTypes correctly.
    #[test]
    fn test_extract_return_type_name_primitive_types() {
        let struct_defs: Vec<StructDefInfo> = vec![];
        assert_eq!(
            extract_return_type_name(&ValueType::I64, &struct_defs),
            Some("Int64".to_string())
        );
        assert_eq!(
            extract_return_type_name(&ValueType::F64, &struct_defs),
            Some("Float64".to_string())
        );
        assert_eq!(
            extract_return_type_name(&ValueType::I32, &struct_defs),
            Some("Int32".to_string())
        );
        // DataType return type is too generic to extract a name
        assert_eq!(
            extract_return_type_name(&ValueType::DataType, &struct_defs),
            None
        );
    }

    /// Integration test for Issue #2908 / #3018:
    /// Verify that the promotion rule extraction pipeline correctly handles struct return types.
    ///
    /// This simulates what `extract_promotion_rules` does for a Julia-defined promote_rule
    /// method that returns a struct type (e.g., `promote_rule(Rational{Int64}, Int64) = Rational{Int64}`).
    ///
    /// Background: The type inference engine infers promote_rule return types as `Any`
    /// (because returning a type-object like `Int64` is not currently tracked precisely).
    /// However, if/when the inference is improved, `ValueType::Struct(id)` will appear
    /// as the return type — and `extract_return_type_name` must correctly resolve it.
    ///
    /// Before bug #2908: `extract_return_type_name` returned `None` for `ValueType::Struct(id)`,
    /// silently dropping these rules. This test verifies the fix remains in place.
    #[test]
    fn test_extract_promotion_rules_pipeline_with_struct_return_type() {
        use crate::types::JuliaType;

        promotion::clear_registry();

        // struct_defs maps Struct(0) -> "Rational{Int64}"
        let struct_defs = vec![StructDefInfo {
            name: "Rational{Int64}".to_string(),
            is_mutable: false,
            fields: vec![
                ("num".to_string(), ValueType::I64),
                ("den".to_string(), ValueType::I64),
            ],
            parent_type: None,
        }];

        // Simulate: promote_rule(::Type{Rational{Int64}}, ::Type{Int64}) = Rational{Int64}
        // This mirrors how extract_promotion_rules processes MethodSig entries.
        let param1 =
            JuliaType::TypeOf(Box::new(JuliaType::Struct("Rational{Int64}".to_string())));
        let param2 = JuliaType::TypeOf(Box::new(JuliaType::Int64));
        let return_vt = ValueType::Struct(0); // Rational{Int64} is at index 0

        // Step 1: extract param types from Type{T} wrappers
        let type1 = extract_type_from_typeof(&param1);
        let type2 = extract_type_from_typeof(&param2);
        assert_eq!(type1, Some("Rational{Int64}".to_string()));
        assert_eq!(type2, Some("Int64".to_string()));

        // Step 2: extract return type name — this is the critical step that was broken in #2908.
        // Before the fix, Struct(id) returned None. After: it resolves via struct_defs.
        let return_type = extract_return_type_name(&return_vt, &struct_defs);
        assert_eq!(
            return_type,
            Some("Rational{Int64}".to_string()),
            "extract_return_type_name must resolve ValueType::Struct(0) to 'Rational{{Int64}}'. \
             If None is returned, the #2908 fix was regressed."
        );

        // Step 3: register the rule and verify it's usable
        if let (Some(t1), Some(t2), Some(ret)) = (type1, type2, return_type) {
            promotion::register_promotion_rule(&t1, &t2, &ret);
        }

        // The rule is now in the registry; promote_type should find it
        let result = promotion::promote_type("Rational{Int64}", "Int64");
        assert_eq!(
            result, "Rational{Int64}",
            "promote_type must return 'Rational{{Int64}}' using the registered rule, not 'Any'"
        );

        promotion::clear_registry();
    }

    /// Tests for the IR-body-based extraction (Issue #3025 fix).
    /// Verifies extract_return_type_from_promote_rule_body handles both body patterns.
    #[test]
    fn test_extract_return_type_from_promote_rule_body_primitive() {
        use crate::ir::core::{Block, Expr, Stmt};
        use crate::span::Span;

        let span = Span {
            start: 0,
            end: 0,
            start_line: 0,
            end_line: 0,
            start_column: 0,
            end_column: 0,
        };
        // Pattern: Stmt::Expr { expr: Var("Int64") } → "Int64"
        let body = Block {
            stmts: vec![Stmt::Expr {
                expr: Expr::Var("Int64".to_string(), span),
                span,
            }],
            span,
        };
        assert_eq!(
            extract_return_type_from_promote_rule_body(&body),
            Some("Int64".to_string()),
            "Var('Int64') body should yield 'Int64'"
        );
    }

    #[test]
    fn test_extract_return_type_from_promote_rule_body_struct() {
        use crate::ir::core::{Block, BuiltinOp, Expr, Literal, Stmt};
        use crate::span::Span;

        let span = Span {
            start: 0,
            end: 0,
            start_line: 0,
            end_line: 0,
            start_column: 0,
            end_column: 0,
        };
        // Pattern: Stmt::Expr { expr: Builtin { TypeOf, [Literal::Str("Complex{Float64}")] } }
        let body = Block {
            stmts: vec![Stmt::Expr {
                expr: Expr::Builtin {
                    name: BuiltinOp::TypeOf,
                    args: vec![Expr::Literal(
                        Literal::Str("Complex{Float64}".to_string()),
                        span,
                    )],
                    span,
                },
                span,
            }],
            span,
        };
        assert_eq!(
            extract_return_type_from_promote_rule_body(&body),
            Some("Complex{Float64}".to_string()),
            "Builtin(TypeOf, [Str('Complex{{Float64}}')]) body should yield 'Complex{{Float64}}'"
        );
    }

    /// Invariant test for Issue #3038: verify that clear_cache() also clears all
    /// associated thread-local registries (not just BASE_CACHE and PROGRAM_CACHE).
    ///
    /// This ensures that the cache and registries are always in sync, preventing
    /// the class of bugs where BASE_CACHE is populated but a registry is empty.
    #[test]
    fn test_clear_cache_also_clears_promotion_registry() {
        // Compile once to populate both cache and registry
        clear_cache();
        let program = parse_and_lower_ok("x = 1");
        compile_with_cache(&program).expect("compilation must succeed");

        assert!(is_cache_initialized(), "cache should be populated after compile");
        assert!(
            promotion::is_registry_initialized(),
            "registry should be populated after compile"
        );
        assert!(
            promotion::get_registry_size() > 0,
            "registry should have rules after compile"
        );

        // clear_cache() must also clear the promotion registry (Issue #3038)
        clear_cache();

        assert!(!is_cache_initialized(), "cache should be cleared");
        assert!(
            !promotion::is_registry_initialized(),
            "promotion registry must be cleared by clear_cache() (Issue #3038). \
             Failing here means clear_cache() does not call promotion::clear_registry()."
        );
        assert_eq!(
            promotion::get_registry_size(),
            0,
            "promotion registry must be empty after clear_cache()"
        );
    }

    /// Regression test for Issue #3025: verify that the promotion rule registry is
    /// actually populated after Base compilation (not empty due to Any return types).
    ///
    /// Before the fix: extract_promotion_rules used method_table return types (all Any),
    /// so 0 rules were ever registered.
    /// After the fix: extract_promotion_rules_from_ir reads from function body expressions,
    /// correctly extracting both primitive and struct return types.
    #[test]
    fn test_promotion_rules_populated_after_base_compilation() {
        // Clear everything to force fresh Base compilation in this thread.
        // clear_cache() also clears the promotion registry (Issue #3038).
        clear_cache();

        let program = parse_and_lower_ok("x = 1");
        compile_with_cache(&program).expect("compilation must succeed");

        // Registry must be initialized
        assert!(
            promotion::is_registry_initialized(),
            "Registry must be initialized after Base compilation"
        );

        // Must have many rules — Base has ~168 concrete promote_rule methods
        let size = promotion::get_registry_size();
        assert!(
            size > 50,
            "Expected >50 promotion rules; got {}. \
             If 0 rules are registered, extract_promotion_rules_from_ir is broken.",
            size
        );

        // Verify a specific Rational rule: promote_rule(Rational{Int64}, Int64) = Rational{Int64}
        // Without the fix, this returned "Any" (Rust fallback; no Julia rule found).
        let result = promotion::promote_type("Rational{Int64}", "Int64");
        assert_eq!(
            result, "Rational{Int64}",
            "promote_type(Rational{{Int64}}, Int64) must return 'Rational{{Int64}}', not 'Any'"
        );

        // Verify symmetric direction
        let result = promotion::promote_type("Int64", "Rational{Int64}");
        assert_eq!(result, "Rational{Int64}");

        // Verify a Complex rule: promote_rule(Complex{Float64}, Complex{Int64}) = Complex{Float64}
        let result = promotion::promote_type("Complex{Float64}", "Complex{Int64}");
        assert_eq!(result, "Complex{Float64}");

        // Verify Int64 + Float64 → Float64 rule
        let result = promotion::promote_type("Int64", "Float64");
        assert_eq!(result, "Float64");
    }

    /// Regression test for Issue #3028: verify that promotion rules survive
    /// the serialize → deserialize roundtrip through the Base cache.
    ///
    /// This prevents the same regression that affected show_methods (Issue #2489):
    /// a registry populated at compile time being silently lost when the cache is
    /// serialized and restored.
    ///
    /// The test simulates the `--precompile-base` → embedded cache path:
    /// 1. Compile Base (populates promotion rule registry)
    /// 2. Export cache + serialize to bytes
    /// 3. Deserialize bytes back
    /// 4. Verify promotion_rules in deserialized cache are non-empty and correct
    #[test]
    fn test_promotion_rules_survive_serialize_deserialize_roundtrip() {
        use crate::compile::precompile::{deserialize_base_cache, serialize_base_cache};

        // Fresh compile to populate registry.
        // clear_cache() also clears the promotion registry (Issue #3038).
        clear_cache();

        let program = parse_and_lower_ok("x = 1");
        compile_with_cache(&program).expect("compilation must succeed");

        // Export the base cache data
        let (compiled, method_tables, closure_captures) =
            export_base_cache().expect("Base cache must be populated after compilation");

        // Serialize to bytes
        let bytes = serialize_base_cache(&compiled, &method_tables, &closure_captures)
            .expect("serialization must succeed");
        assert!(!bytes.is_empty(), "Serialized cache must be non-empty");

        // Deserialize back
        let restored = deserialize_base_cache(&bytes)
            .expect("deserialization must succeed with valid bytes");

        // Verify promotion rules are non-empty in the restored cache
        assert!(
            !restored.promotion_rules.is_empty(),
            "Deserialized cache must contain promotion rules. \
             Got 0 rules — serialize_base_cache is not capturing the registry. \
             See Issue #3025 and #3028."
        );

        // Must have many rules (Base defines ~168 concrete promote_rule methods)
        assert!(
            restored.promotion_rules.len() > 50,
            "Expected >50 promotion rules in deserialized cache, got {}",
            restored.promotion_rules.len()
        );

        // Verify specific rules are present
        let has_int64_float64 = restored
            .promotion_rules
            .iter()
            .any(|(t1, t2, ret)| t1 == "Int64" && t2 == "Float64" && ret == "Float64"
                || t1 == "Float64" && t2 == "Int64" && ret == "Float64");
        assert!(
            has_int64_float64,
            "Deserialized cache must contain the Int64+Float64→Float64 promotion rule"
        );

        // Verify roundtrip: replay rules into a fresh registry and test lookup
        promotion::clear_registry();
        for (t1, t2, ret) in &restored.promotion_rules {
            promotion::register_promotion_rule(t1, t2, ret);
        }
        promotion::mark_registry_initialized();

        let result = promotion::promote_type("Int64", "Float64");
        assert_eq!(
            result, "Float64",
            "After replaying deserialized rules, promote_type(Int64, Float64) must return Float64"
        );

        let result = promotion::promote_type("Rational{Int64}", "Int64");
        assert_eq!(
            result, "Rational{Int64}",
            "After replaying deserialized rules, promote_type(Rational{{Int64}}, Int64) must return Rational{{Int64}}"
        );

        // Restore registry to avoid interfering with other tests
        promotion::clear_registry();
    }

    /// Verify that running compile_with_cache twice (second run uses the cached Base)
    /// also results in a populated promotion rule registry.
    ///
    /// The second run skips Base compilation and restores from the thread-local cache.
    /// This tests that the registry is consistently available regardless of whether
    /// the Base was compiled from scratch or from cache.
    #[test]
    fn test_promotion_rules_populated_on_second_compile_with_cache() {
        // First compile: fresh Base compilation.
        // clear_cache() also clears the promotion registry (Issue #3038).
        clear_cache();

        let program = parse_and_lower_ok("x = 1");
        compile_with_cache(&program).expect("first compile must succeed");
        let size_after_first = promotion::get_registry_size();
        assert!(
            size_after_first > 50,
            "Registry must be populated after first compile, got {}",
            size_after_first
        );

        // Clear the promotion registry but NOT the base cache —
        // this simulates a fresh thread or re-entrant compilation
        promotion::clear_registry();
        assert_eq!(promotion::get_registry_size(), 0);

        // Second compile: Base is already compiled (cache hit), but registry was cleared.
        // The cache machinery must re-populate the registry from cached data.
        let program2 = parse_and_lower_ok("y = 2");
        compile_with_cache(&program2).expect("second compile must succeed");

        // Registry should be re-populated from the Base cache
        let size_after_second = promotion::get_registry_size();
        assert!(
            size_after_second > 50,
            "Registry must be re-populated from cache on second compile, got {}. \
             This would indicate the cache replay path is not restoring promotion rules.",
            size_after_second
        );

        // Restore
        promotion::clear_registry();
    }
}
