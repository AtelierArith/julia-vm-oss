//! Named pipeline phases for `compile_core_program_internal` (Issue #6333).
//!
//! Mirrors upstream Julia's `run_passes_ipo_safe` (julia/Compiler/src/optimize.jl)
//! where the compilation pipeline is an explicit sequence of named phases.

use super::method_table::ConstructorSelfFamily;
use super::*;
use crate::compile::context::StructRegistry;
use crate::compile::context::TypeDefinitionPosition;
use crate::compile::type_helpers::julia_type_to_value_type_scoped;
use crate::ir::core::RuntimeNominalDef;
use crate::runtime_types::bridge;
use subset_julia_vm_bytecode::{
    stack_backend, EnumDefInfo, ModuleOperands, SpecializationDisableFlags,
};

fn collect_enum_def_infos(block: &Block, output: &mut Vec<EnumDefInfo>) {
    for statement in &block.stmts {
        match statement {
            Stmt::EnumDef { enum_def, .. } => output.push(EnumDefInfo {
                name: enum_def.name.clone(),
                base_type: enum_def.base_type.clone(),
                members: enum_def
                    .members
                    .iter()
                    .map(|member| (member.name.clone(), member.value))
                    .collect(),
            }),
            Stmt::Block(inner) => collect_enum_def_infos(inner, output),
            _ => {}
        }
    }
}

#[cfg(test)]
mod runtime_inner_constructor_collection_11679_tests {
    use super::*;

    #[test]
    fn collects_runtime_inner_structs_but_skips_const_dead_declarations_11679() {
        let Ok(program) = crate::pipeline::parse_and_lower(
            r#"
runtime_condition11679() = true
if runtime_condition11679()
    struct CollectedRuntimeInner11679
        x::Int
        CollectedRuntimeInner11679(x) = new(x + 1)
    end
end
if false
    struct DeadRuntimeInner11679
        x::Int
        DeadRuntimeInner11679(x) = new(x + 1)
    end
end
"#,
        ) else {
            unreachable!("runtime inner constructor source must parse/lower");
        };
        let mut collected = Vec::new();
        collect_runtime_inner_constructor_structs_in_block(&program.main, None, &mut collected);
        assert_eq!(
            collected
                .iter()
                .map(|(definition, _)| definition.name.as_str())
                .collect::<Vec<_>>(),
            vec!["CollectedRuntimeInner11679"]
        );
    }
}

/// Select the current REPL fragment's Julia-visible source methods by
/// structural provenance, not by a contiguous function-table prefix.
///
/// Lowering helpers may be interposed anywhere in the top-level user region and
/// carry explicit reserved provenance; they never consume the source-method count.
/// Prior methods follow the selected source count and stay immediately visible.
fn repl_current_input_source_function_indices(
    all_functions: &[(&Function, Option<String>)],
    first_user_function_idx: usize,
    inline_start_idx: usize,
    func_idx_to_parent: &HashMap<usize, String>,
    source_function_count: Option<usize>,
) -> Option<HashSet<usize>> {
    source_function_count.map(|count| {
        all_functions
            .iter()
            .enumerate()
            .filter(|(index, (function, module_path))| {
                *index >= first_user_function_idx
                    && *index < inline_start_idx
                    && module_path.is_none()
                    && !func_idx_to_parent.contains_key(index)
                    && !crate::compile::ir_inline::is_markerless_lowered_function(function)
            })
            .take(count)
            .map(|(index, _)| index)
            .collect()
    })
}

/// Names of the parameters whose slot must stay generic because their declared
/// type is an abstract numeric supertype that can hold a `BigInt`/`BigFloat`
/// value (`Integer`/`Real`/`Number`/`Signed`/`Unsigned`/`AbstractFloat`, or a
/// `where` type-var bounded by one of those). Their annotation maps to a machine
/// `ValueType` (`I64`/`F64`) for dispatch/return typing, so without this the
/// slot would carry a machine tag and the slotizer could upgrade a stray typed
/// load into a `LoadSlotI64`/`LoadSlotF64` that rejects the wide runtime value.
/// Mirrors how the compiler populates `abstract_numeric_params` (Issue #9724).
fn abstract_numeric_param_slot_names(func_info: &FunctionInfo) -> HashSet<String> {
    let mut names = HashSet::new();
    for ((name, _), jt) in func_info
        .params
        .iter()
        .zip(func_info.param_julia_types.iter())
    {
        if param_type_is_abstract_numeric(jt) {
            names.insert(name.clone());
        }
    }
    names
}

/// True when `jt` is an abstract numeric supertype that admits `BigInt`/
/// `BigFloat` — either directly (`x::Integer`) or as a `where` bound
/// (`x::T where {T<:Real}`). Deliberately excludes the fixed-width `Union`
/// aliases (`BitSigned`/`BitUnsigned`), whose members are all machine scalars,
/// so their typed slots stay fast (Issue #9724).
fn param_type_is_abstract_numeric(jt: &JuliaType) -> bool {
    if jt.is_abstract_numeric() {
        return true;
    }
    if let JuliaType::TypeVar(_, Some(bound)) = jt {
        if let Some(bound_ty) = JuliaType::from_name(bound) {
            return bound_ty.is_abstract_numeric();
        }
    }
    false
}

fn param_type_is_display_io(jt: &JuliaType) -> bool {
    match jt {
        JuliaType::IO => true,
        JuliaType::Struct(name) | JuliaType::AbstractUser(name, _) => {
            let head = name.split_once('{').map_or(name.as_str(), |(head, _)| head);
            head == "IOContext" || head.ends_with(".IOContext")
        }
        _ => false,
    }
}

/// True when `func`'s body is trivial enough that the SSA pipeline (Issue
/// #8552: build → const-fold/CSE/DCE passes → plan) cannot improve on it —
/// there is nothing to fold, eliminate, or reorder in a single-statement body
/// whose only call/return arguments are literals (Issue #10115). Matches:
/// a single `Call(builtin_or_user_name, literal_args)` (no kwargs/splat), a
/// bare `return <literal>` / implicit-tail literal, or an empty `return`.
/// Also requires no `where` type parameters and no keyword parameters,
/// mirroring the SSA path's own eligibility gate (kwparams already force a
/// legacy fallback there) so this is a strict subset of "would have gone
/// through SSA anyway", not a new class of body.
///
/// This does not change WHAT gets compiled, only WHICH path compiles it: a
/// body that matches goes straight to `CoreCompiler::compile_function_body`,
/// the exact legacy path every Base/prelude function (and any SSA-ineligible
/// user function) already takes.
fn is_trivial_ssa_fast_path_body(func: &Function) -> bool {
    if !func.type_params.is_empty() || !func.kwparams.is_empty() {
        return false;
    }
    let [stmt] = func.body.stmts.as_slice() else {
        return false;
    };

    fn is_trivial_call(expr: &Expr) -> bool {
        matches!(
            expr,
            Expr::Call {
                kwargs,
                splat_mask,
                kwargs_splat_mask,
                args,
                ..
            } if kwargs.is_empty()
                && kwargs_splat_mask.iter().all(|splat| !splat)
                && splat_mask.iter().all(|splat| !splat)
                && args.iter().all(|arg| matches!(arg, Expr::Literal(..)))
        )
    }

    match stmt {
        Stmt::Return { value: None, .. } => true,
        Stmt::Return {
            value: Some(expr), ..
        } => matches!(expr, Expr::Literal(..)) || is_trivial_call(expr),
        Stmt::Expr { expr, .. } => matches!(expr, Expr::Literal(..)) || is_trivial_call(expr),
        _ => false,
    }
}

/// Named result of `compile_core_program_internal`, replacing the previous
/// 4-element tuple (Issue #6333).
#[derive(Debug)]
pub(crate) struct CoreCompileOutput {
    pub compiled: CompiledProgram,
    pub method_tables: HashMap<String, MethodTable>,
    pub closure_captures: HashMap<String, HashSet<String>>,
    pub inference_results: Vec<(InferenceCacheKey, CachedReturn)>,
    /// Every method row in source registration order, including rows replaced
    /// by a later same-signature definition. REPL error recovery needs the
    /// reached row, which cannot be reconstructed from the final method table.
    pub(crate) source_ordered_method_sigs:
        HashMap<String, Vec<super::context::SourceOrderedMethodSig>>,
    /// Module path (e.g. `"LinearAlgebra"`, `"Plots.RecipesBase"`) -> the
    /// `(compiled.functions index, bare IR function name)` pairs of that
    /// module's directly-defined functions (Issue #9189). Built from the same
    /// `all_functions`/`func_index_map` bookkeeping `compile_functions`
    /// already relies on for the cached-Base fast path, so it reflects ground
    /// truth rather than a name-based re-derivation (which would misattribute
    /// overloads shared across modules).
    ///
    /// The bare name is recorded explicitly rather than read back off
    /// `compiled.functions[idx].name`: for any module-scoped function,
    /// `FunctionInfo.name` is *always* module-qualified (`format!("{}.{}",
    /// module_path, func.name)`, see this file's `function_name` construction
    /// a few hundred lines below) so `methods`/reflection can disambiguate —
    /// but the IR `Function.name` a not-yet-compiled Stage 2 lookup site sees
    /// is bare. Recording the bare name here once (where the original
    /// `&Function` is already in scope) avoids needing to reverse-engineer
    /// the qualification convention at every consumer.
    ///
    /// Consumed only by `compile::preload_cache::generate_preload_cache_for`
    /// (build-time cache generation) to slice out a module's compiled
    /// function bodies; the ordinary compile path's Stage 2 lookup instead
    /// reuses `build_method_tables`'s already-resolved `params` local
    /// directly (see `preload_cache::signature_key_for_resolved_params`), so
    /// this field stays unread there. Real builds only call
    /// `generate_preload_cache_for` from the (not-yet-added)
    /// `--precompile-packages` CLI path, so this is genuinely unread outside
    /// that tooling path and test builds today.
    #[allow(dead_code)]
    pub module_function_infos: HashMap<String, Vec<(usize, String)>>,
    /// The non-Base function layout in `all_functions` (== global function
    /// index) order: `(module_path, bare IR name)` for each function at index
    /// `base_function_count + i` (Issue #9230). This is the whole-prefix-reuse
    /// gate for the preloaded-package cache: the cache is only reused when the
    /// consuming program's non-Base prefix *starts with* the cached closure's
    /// layout, so every spliced body's frozen absolute function index still
    /// points at the same function (layout identity — no relocation). The layout
    /// spans the WHOLE non-Base region (`base_function_count..`), including the
    /// trailing lifted Base closures a spliced body can reach (Issue #9254);
    /// any user `function`, user body closure, or main lifted lambda that shifts
    /// that region fails the prefix match and deactivates the gate (fail-safe).
    /// Programs that add nothing after the deterministic Base-closure tail
    /// (e.g. the iOS `using ...; plot([1,2,3])` samples) stay aligned. Read only
    /// by `preload_cache`.
    #[allow(dead_code)]
    pub nonbase_layout: Vec<(Option<String>, String)>,
    /// How many preloaded-package function bodies were spliced in this compile
    /// (Issue #9230). `0` means the preload cache was absent or its
    /// `closure_layout` gate deactivated it; `> 0` confirms the whole-prefix
    /// reuse actually fired. Read by tests / precompile tooling.
    #[allow(dead_code)]
    pub preload_spliced_count: usize,
    /// Number of cache-covered Base/prelude functions outside the flat
    /// `0..base_function_count` prefix that were mapped back to cached
    /// `FunctionInfo`s instead of rebuilt (Issue #10211).
    #[allow(dead_code)]
    pub cached_base_extra_reused_count: usize,
    /// Absolute code offset where the USER main block begins (after the reused
    /// prefix, freshly compiled function bodies, base-main prefix, and any
    /// modules), in FINAL post-peephole coordinates (Issue #9199 LV2). Only
    /// meaningful — and only guaranteed to be a clean extraction boundary (a
    /// peephole barrier is installed at the seam) — when the compile was driven
    /// with `CompilerCacheInput::global_slot_seed = Some(..)`; `None` otherwise.
    /// The REPL live-append path slices `compiled.code[user_main_entry..]` as the
    /// relocatable delta main.
    pub user_main_entry: Option<usize>,
}

/// Map a single code index through a peephole `old_to_new` mapping, mirroring
/// how `apply_peephole_index_mapping` remaps `entry` (Issue #9199 LV2). Used to
/// carry `user_main_entry` through the same peephole passes as `entry`.
fn map_index_through(idx: usize, index_mapping: &[usize]) -> usize {
    if idx < index_mapping.len() {
        index_mapping[idx]
    } else {
        idx
    }
}

fn apply_peephole_index_mapping(
    function_infos: &mut [std::rc::Rc<FunctionInfo>],
    entry: usize,
    index_mapping: &[usize],
    reused_base: &[bool],
) -> usize {
    for (idx, func_info) in function_infos.iter_mut().enumerate() {
        if reused_base.get(idx).copied().unwrap_or(false) {
            continue;
        }
        // Non-reused entries are freshly compiled this run (refcount 1), so
        // make_mut mutates in place without cloning (Issue #9140).
        let func_info = std::rc::Rc::make_mut(func_info);
        if func_info.code_start < index_mapping.len() {
            func_info.code_start = index_mapping[func_info.code_start];
        }
        if func_info.code_end < index_mapping.len() {
            func_info.code_end = index_mapping[func_info.code_end];
        }
        if func_info.entry < index_mapping.len() {
            func_info.entry = index_mapping[func_info.entry];
        }
    }

    if entry < index_mapping.len() {
        index_mapping[entry]
    } else {
        entry
    }
}

fn apply_peephole_source_map(
    source_map: Vec<Option<crate::span::Span>>,
    index_mapping: &[usize],
) -> Vec<Option<crate::span::Span>> {
    let new_len = index_mapping.last().copied().unwrap_or(source_map.len());
    let mut mapped = vec![None; new_len];
    for (old_idx, span) in source_map.into_iter().enumerate() {
        let Some(new_idx) = index_mapping.get(old_idx).copied() else {
            continue;
        };
        if new_idx < mapped.len() && mapped[new_idx].is_none() {
            mapped[new_idx] = span;
        }
    }
    mapped
}

/// Refresh the compile-time-frozen dispatch candidate lists inside cached
/// Base bytecode with the user program's methods for the retired Base-cache
/// hooks (Issue #8555, slice of #8442).
///
/// Cached Base bytecode is reused verbatim on the precompiled-Base path, so
/// its named dynamic-dispatch sites (`CallTypedDispatch`-family and
/// `PushResolvedFunction`) still carry the candidate method indices that
/// existed when Base itself was compiled. A user method added to one of those
/// generic functions (e.g. `promote_rule`) is therefore invisible to cached
/// call sites — the reason Issue #4048 originally disabled the whole Base
/// cache. Instead of recompiling Base, append the user methods' global
/// indices to the affected candidate lists, reproducing exactly what a full
/// recompile's emitters (`compile/expr/call/dispatch.rs`) would bake in:
/// candidates are the method-table entries accepting the site's arity, in
/// table order (Base methods first, user methods appended).
///
/// Only names in [`super::cache::BASE_DISPATCH_REFRESH_HOOKS`] are refreshed;
/// the other hook names still take the full-compile bypass in
/// `cache::should_skip_base_cache_for_program` (deferred slices of #8555).
fn refresh_cached_base_dispatch_candidates(
    base_code: &mut [Instr],
    method_tables: &HashMap<String, MethodTable>,
    function_infos: &[std::rc::Rc<FunctionInfo>],
    base_function_count: usize,
) {
    let user_methods_by_hook: HashMap<&str, Vec<&MethodSig>> =
        super::cache::BASE_DISPATCH_REFRESH_HOOKS
            .iter()
            .filter_map(|&hook| {
                let user_methods: Vec<&MethodSig> = method_tables
                    .get(hook)?
                    .methods
                    .iter()
                    .filter(|method| !method.is_base_program_method(base_function_count))
                    .collect();
                (!user_methods.is_empty()).then_some((hook, user_methods))
            })
            .collect();
    if user_methods_by_hook.is_empty() {
        return;
    }

    // Instruction names can be module-qualified (`Base.promote_rule`); hooks
    // are bare Base names, so match on the final name segment.
    let hook_methods_for = |name: &str| {
        let base_name = name.rsplit('.').next().unwrap_or(name);
        user_methods_by_hook.get(base_name)
    };
    // `CallDynamic[OrBuiltin]` payloads carry no function name; recover the
    // generic-function identity from a referenced function index (the
    // fallback or any resolved method candidate — emit sites draw all of them
    // from one method table).
    let hook_methods_for_index = |func_index: usize| {
        function_infos
            .get(func_index)
            .and_then(|info| hook_methods_for(&info.name))
    };
    let append_candidates =
        |candidates: &mut Vec<usize>, methods: &[&MethodSig], arity: Option<usize>| {
            for method in methods {
                if arity.is_some_and(|arg_count| !method.accepts_arity(arg_count)) {
                    continue;
                }
                if !candidates.contains(&method.global_index) {
                    candidates.push(method.global_index);
                }
            }
        };

    for instr in base_code {
        match instr {
            Instr::CallTypedDispatch(name, arg_count, _, candidates) => {
                if let Some(methods) = hook_methods_for(name) {
                    append_candidates(candidates, methods, Some(*arg_count));
                }
            }
            Instr::CallTypedDispatchOrBuiltin(_, name, arg_count, candidates)
            | Instr::CallTypedDispatchOrBuiltinResult(_, name, arg_count, candidates) => {
                if let Some(methods) = hook_methods_for(name) {
                    append_candidates(candidates, methods, Some(*arg_count));
                }
            }
            Instr::CallTypedDispatchOrBuiltinStoreDict(operands)
            | Instr::CallTypedDispatchOrBuiltinStoreDictResult(operands) => {
                if let Some(methods) = hook_methods_for(&operands.function_name) {
                    let arg_count = operands.arg_count;
                    append_candidates(&mut operands.candidates, methods, Some(arg_count));
                }
            }
            Instr::PushResolvedFunction(operands) => {
                if let Some(methods) = hook_methods_for(&operands.name) {
                    append_candidates(&mut operands.candidate_indices, methods, None);
                }
            }
            Instr::CallDynamic(operands) => {
                let methods = hook_methods_for(&operands.callee_name)
                    .or_else(|| hook_methods_for_index(operands.fallback_func_index))
                    .or_else(|| {
                        operands
                            .candidates
                            .iter()
                            .find_map(|candidate| match candidate {
                                DynamicCallCandidate::Method(idx) => hook_methods_for_index(*idx),
                                DynamicCallCandidate::NativeIterator(_) => None,
                            })
                    });
                if let Some(methods) = methods {
                    for method in methods {
                        if !method.accepts_arity(operands.arg_count) {
                            continue;
                        }
                        let entry = DynamicCallCandidate::Method(method.global_index);
                        if !operands.candidates.contains(&entry) {
                            operands.candidates.push(entry);
                        }
                    }
                }
            }
            Instr::CallDynamicOrBuiltin(_, candidates) => {
                // Unary value dispatch with builtin fallback; candidates all
                // belong to one generic function.
                let methods = candidates
                    .iter()
                    .find_map(|&idx| hook_methods_for_index(idx));
                if let Some(methods) = methods {
                    append_candidates(candidates, methods, Some(1));
                }
            }
            _ => {}
        }
    }
}

/// Struct tables built by [`CorePipeline::build_struct_tables`] and consumed
/// by [`CorePipeline::init_shared_context`] when creating the shared
/// compilation context.
struct StructTables {
    struct_table: StructRegistry,
    parametric_structs: HashMap<String, ParametricStructDef>,
    base_parametric_structs: HashMap<String, ParametricStructDef>,
    struct_defs: Vec<StructDefInfo>,
    next_type_id: usize,
    cached_instantiation_table: HashMap<InstantiationKey, usize>,
}

/// An inner constructor collected from a struct definition. Registered with
/// the struct name, allowing `Point(x, y)` to call the inner constructor.
#[derive(Debug, Clone, PartialEq, Eq)]
enum InnerCtorTarget {
    Concrete { type_id: usize },
    Parametric { qualified_name: String },
}

struct InnerCtorInfo {
    /// The declaration-owned allocation target. Concrete ids and parametric
    /// names are mutually exclusive, so id zero remains a valid concrete id
    /// and module-owned parametric constructors keep their qualified owner.
    target: InnerCtorTarget,
    is_base_origin: bool,
    ctor: crate::ir::core::InnerConstructor,
    func_info_idx: usize, // Index in function_infos where this ctor is registered
    /// Dotted path of the module that defines this struct (`None` at top level).
    /// The constructor body's name lookups must be resolved in this defining
    /// module so a module-private helper/type/const is visible without the
    /// caller doing `using .Mod` (Issue #8069).
    module_path: Option<String>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum SyntheticConstructorKind {
    DefaultInner(ConstructorSelfFamily),
    DefaultOuter,
}

#[derive(Debug, Clone)]
struct SyntheticConstructorMethod {
    kind: SyntheticConstructorKind,
    ctor: crate::ir::core::InnerConstructor,
}

impl SyntheticConstructorMethod {
    fn constructor_self_family(&self) -> Option<ConstructorSelfFamily> {
        match self.kind {
            SyntheticConstructorKind::DefaultInner(family) => Some(family),
            SyntheticConstructorKind::DefaultOuter => None,
        }
    }
}

type StructOriginEntry<'a> = (&'a crate::ir::core::StructDef, Option<String>, bool);

/// A Main-owned definition whose binding is published when execution reaches
/// its source position. Keeping functions and concrete structs in one queue is
/// essential: an errored REPL input commits one exact interleaved prefix, not
/// independent per-kind counts (Issues #9784/#11546).
enum TopLevelDefinitionActivation {
    Function {
        source_start: usize,
        definition_order: u64,
        func_idx: usize,
        type_params: Vec<crate::types::TypeParam>,
        params: Vec<crate::ir::core::TypedParam>,
        kwparams: Vec<crate::ir::core::KwParam>,
    },
    Struct {
        source_start: usize,
        definition_order: u64,
        type_name: String,
        type_id: usize,
    },
    AbstractType {
        source_start: usize,
        definition_order: u64,
        type_name: String,
        type_id: usize,
    },
    PrimitiveType {
        source_start: usize,
        definition_order: u64,
        type_name: String,
        type_id: usize,
    },
}

impl TopLevelDefinitionActivation {
    fn source_start(&self) -> usize {
        match self {
            Self::Function { source_start, .. }
            | Self::Struct { source_start, .. }
            | Self::AbstractType { source_start, .. }
            | Self::PrimitiveType { source_start, .. } => *source_start,
        }
    }

    fn definition_order(&self) -> u64 {
        match self {
            Self::Function {
                definition_order, ..
            }
            | Self::Struct {
                definition_order, ..
            }
            | Self::AbstractType {
                definition_order, ..
            }
            | Self::PrimitiveType {
                definition_order, ..
            } => *definition_order,
        }
    }

    fn kind_rank(&self) -> u8 {
        match self {
            Self::Struct { .. } | Self::AbstractType { .. } | Self::PrimitiveType { .. } => 0,
            Self::Function { .. } => 1,
        }
    }
}

fn compact_type_name(name: &str) -> String {
    name.chars().filter(|c| !c.is_whitespace()).collect()
}

/// Qualified `Base.<alias>` DataType target for Base-internal const type
/// aliases (Issue #10579): upstream resolves the QUALIFIED spelling
/// (`Base.BitSigned`, `Base.Bottom`) even though the aliases are unexported.
/// Resolution goes against the PRELUDE's own alias list — not the flat
/// shared table, which also holds user aliases that must not become
/// reachable as `Base.X` — and recomputes the target from the prelude
/// definition so a same-named user alias cannot shadow the Base binding.
/// `Bottom` is qualified-only by design: a flat alias would leak the bare
/// name into Main (Issues #10304/#10578), so it maps here directly.
pub(crate) fn prelude_base_type_alias_target(function: &str) -> Option<String> {
    if function == "Bottom" {
        return Some("Union{}".to_string());
    }
    let prelude = crate::get_prelude_program()?;
    prelude
        .type_aliases
        .iter()
        .find(|alias| alias.name == function)
        .map(type_alias_runtime_target)
}

pub(crate) fn type_alias_runtime_target(alias: &crate::ir::core::TypeAliasDef) -> String {
    if alias.params.is_empty() {
        alias.target_type.clone()
    } else {
        match alias.target_type.split_once('{') {
            Some((base, _)) => base.trim().to_string(),
            None => alias.target_type.clone(),
        }
    }
}

fn type_expr_contains_type_param(expr: &TypeExpr, type_param_names: &HashSet<&str>) -> bool {
    match expr {
        TypeExpr::TypeVar(name) => type_param_names.contains(name.as_str()),
        TypeExpr::Parameterized { params, .. } => params
            .iter()
            .any(|param| type_expr_contains_type_param(param, type_param_names)),
        TypeExpr::Concrete(_) | TypeExpr::RuntimeExpr(_) => false,
    }
}

fn collect_type_params_from_type_expr(
    expr: &TypeExpr,
    declared: &HashSet<&str>,
    found: &mut HashSet<String>,
) {
    match expr {
        TypeExpr::TypeVar(name) => {
            if declared.contains(name.as_str()) {
                found.insert(name.clone());
            }
        }
        TypeExpr::Parameterized { params, .. } => {
            for param in params {
                collect_type_params_from_type_expr(param, declared, found);
            }
        }
        TypeExpr::Concrete(_) | TypeExpr::RuntimeExpr(_) => {}
    }
}

fn all_struct_type_params_inferable(struct_def: &crate::ir::core::StructDef) -> bool {
    let declared: HashSet<&str> = struct_def
        .type_params
        .iter()
        .map(|param| param.name.as_str())
        .collect();
    let mut inferable = HashSet::new();
    for field in &struct_def.fields {
        if let Some(field_type) = &field.type_expr {
            collect_type_params_from_type_expr(field_type, &declared, &mut inferable);
        }
    }

    // `jl_ctor_def` walks the parameters in reverse: once a parameter is
    // constrained by a field, variables occurring in that parameter's upper
    // bound are constrained too. Iterate to a fixed point so a longer bound
    // dependency chain is handled structurally rather than only the immediate
    // `S,T<:AbstractArray{S}` case (Issue #11147).
    loop {
        let before = inferable.len();
        for type_param in struct_def.type_params.iter().rev() {
            if !inferable.contains(&type_param.name) {
                continue;
            }
            let Some(upper_bound) = type_param.get_upper_bound() else {
                continue;
            };
            if let Some(bound_expr) = crate::types::parse_single_type_expr(upper_bound) {
                collect_type_params_from_type_expr(&bound_expr, &declared, &mut inferable);
            }
        }
        if inferable.len() == before {
            break;
        }
    }

    struct_def
        .type_params
        .iter()
        .all(|type_param| inferable.contains(&type_param.name))
}

fn synthetic_param_julia_type(
    type_expr: Option<&TypeExpr>,
    type_params: &[TypeParam],
) -> JuliaType {
    match type_expr {
        None => JuliaType::Any,
        Some(TypeExpr::TypeVar(name)) => {
            let upper_bound = type_params
                .iter()
                .find(|param| param.name == *name)
                .and_then(TypeParam::get_upper_bound)
                .cloned();
            JuliaType::TypeVar(name.clone(), upper_bound)
        }
        Some(expr) => expr.to_julia_type_lossy(),
    }
}

fn synthetic_type_expr_value(
    expr: &TypeExpr,
    span: crate::span::Span,
    module_path: Option<&str>,
    module_struct_names: &HashMap<String, HashSet<String>>,
) -> CResult<Expr> {
    match expr {
        TypeExpr::TypeVar(name) => Ok(Expr::var(name.clone(), span)),
        TypeExpr::Parameterized { base, params } => {
            let local_qualified_base =
                module_path
                    .filter(|_| !base.contains('.'))
                    .and_then(|path| {
                        module_struct_names
                            .get(path)
                            .is_some_and(|structs| structs.contains(base))
                            .then(|| format!("{path}.{base}"))
                    });
            let (resolved_base, base_expr) = if base.contains('.') {
                (base.clone(), None)
            } else if let Some(qualified) = local_qualified_base {
                (qualified, None)
            } else {
                // A bare non-local base may be imported or re-exported into
                // the struct's defining module. Compile it as a lexical value
                // lookup so module_imported_bindings chooses the exact owner;
                // a literal ConstructParametricType base would fall back to an
                // ambiguous suffix match when another module defines the same
                // short name (Issue #11147).
                (base.clone(), Some(Box::new(Expr::var(base.clone(), span))))
            };
            Ok(Expr::DynamicTypeConstruct {
                base: resolved_base.into(),
                base_expr,
                type_args: params
                    .iter()
                    .map(|param| {
                        synthetic_type_expr_value(param, span, module_path, module_struct_names)
                    })
                    .collect::<CResult<Vec<_>>>()?,
                splat_mask: vec![false; params.len()],
                span,
            })
        }
        TypeExpr::Concrete(_) | TypeExpr::RuntimeExpr(_) => {
            crate::lowering::lower_expr_from_text(&expr.to_string()).map_err(|err| {
                CompileError::Msg(format!(
                    "failed to lower synthesized constructor field type `{expr}`: {err}"
                ))
            })
        }
    }
}

fn fresh_synthetic_name(
    prefix: &str,
    field_index: usize,
    reserved: &mut HashSet<String>,
) -> String {
    let mut suffix = 0usize;
    loop {
        let candidate = format!("__sjulia_{prefix}_{field_index}_{suffix}");
        if reserved.insert(candidate.clone()) {
            return candidate;
        }
        suffix += 1;
    }
}

fn synthetic_default_constructors(
    struct_def: &crate::ir::core::StructDef,
    module_path: Option<&str>,
    module_struct_names: &HashMap<String, HashSet<String>>,
) -> CResult<Vec<SyntheticConstructorMethod>> {
    if !struct_def.inner_constructors.is_empty() {
        return Ok(Vec::new());
    }

    // Workaround: keep synthesized default constructor rows user-only. (Issue #11062)
    if struct_def.is_base_origin {
        return Ok(Vec::new());
    }

    let span = struct_def.span;
    let mut reserved: HashSet<String> = struct_def
        .fields
        .iter()
        .map(|field| field.name.clone())
        .chain(
            struct_def
                .type_params
                .iter()
                .map(|param| param.name.clone()),
        )
        .collect();
    let param_names: Vec<String> = (0..struct_def.fields.len())
        .map(|field_index| fresh_synthetic_name("default_ctor_arg", field_index, &mut reserved))
        .collect();
    let value_exprs: Vec<Expr> = param_names
        .iter()
        .map(|name| Expr::var(name.clone(), span))
        .collect();

    let outer_params: Vec<crate::ir::core::TypedParam> = struct_def
        .fields
        .iter()
        .zip(param_names.iter())
        .map(|(field, name)| {
            crate::ir::core::TypedParam::new(
                name.clone(),
                Some(synthetic_param_julia_type(
                    field.type_expr.as_ref(),
                    &struct_def.type_params,
                )),
                field.span,
            )
        })
        .collect();

    let mut methods = Vec::new();
    if struct_def.type_params.is_empty() {
        methods.push(SyntheticConstructorMethod {
            kind: SyntheticConstructorKind::DefaultOuter,
            ctor: crate::ir::core::InnerConstructor {
                params: outer_params,
                kwparams: Vec::new(),
                type_params: Vec::new(),
                is_explicit_parametric: false,
                explicit_type_parameter_names: Vec::new(),
                explicit_type_arguments: Vec::new(),
                body: Block {
                    stmts: vec![Stmt::Return {
                        value: Some(Expr::New {
                            type_args: Vec::new(),
                            args: value_exprs.clone(),
                            is_splat: false,
                            span,
                        }),
                        span,
                    }],
                    span,
                },
                span,
            },
        });

        let all_fields_any = struct_def.fields.iter().all(|field| {
            field
                .type_expr
                .as_ref()
                .is_none_or(|expr| matches!(expr, TypeExpr::Concrete(JuliaType::Any)))
        });
        if all_fields_any {
            return Ok(methods);
        }
    } else {
        let all_type_params_inferable = all_struct_type_params_inferable(struct_def);
        if all_type_params_inferable {
            methods.push(SyntheticConstructorMethod {
                kind: SyntheticConstructorKind::DefaultOuter,
                ctor: crate::ir::core::InnerConstructor {
                    params: outer_params,
                    kwparams: Vec::new(),
                    type_params: struct_def.type_params.clone(),
                    is_explicit_parametric: false,
                    explicit_type_parameter_names: Vec::new(),
                    explicit_type_arguments: Vec::new(),
                    body: Block {
                        stmts: vec![Stmt::Return {
                            // Upstream `jl_outer_ctor_body` allocates the inferred
                            // concrete type directly. Calling `Foo{T}(...)` here
                            // would re-dispatch through explicit-self methods that
                            // were defined after the struct and let them hijack a
                            // bare `Foo(...)` default-constructor call (Issue #11147).
                            value: Some(Expr::New {
                                type_args: struct_def
                                    .type_params
                                    .iter()
                                    .map(|param| TypeExpr::TypeVar(param.name.clone()))
                                    .collect(),
                                args: value_exprs.clone(),
                                is_splat: false,
                                span,
                            }),
                            span,
                        }],
                        span,
                    },
                    span,
                },
            });
        }
    }

    let mut converted_values = Vec::with_capacity(struct_def.fields.len());
    for (field_index, (field, param_name)) in
        struct_def.fields.iter().zip(param_names.iter()).enumerate()
    {
        let Some(field_type) = field.type_expr.as_ref() else {
            converted_values.push(Expr::var(param_name.clone(), span));
            continue;
        };
        if matches!(field_type, TypeExpr::Concrete(JuliaType::Any)) {
            converted_values.push(Expr::var(param_name.clone(), span));
            continue;
        }

        let target_name = fresh_synthetic_name("default_ctor_target", field_index, &mut reserved);
        let value_name = fresh_synthetic_name("default_ctor_value", field_index, &mut reserved);
        let target = Expr::var(target_name.clone(), span);
        let value = Expr::var(value_name.clone(), span);
        converted_values.push(Expr::LetBlock {
            bindings: vec![
                (
                    target_name.into(),
                    synthetic_type_expr_value(
                        field_type,
                        field.span,
                        module_path,
                        module_struct_names,
                    )?,
                ),
                (value_name.into(), Expr::var(param_name.clone(), span)),
            ],
            body: Block {
                stmts: vec![Stmt::Expr {
                    expr: Expr::Ternary {
                        condition: Box::new(Expr::call(
                            "isa",
                            vec![value.clone(), target.clone()],
                            span,
                        )),
                        then_expr: Box::new(value.clone()),
                        else_expr: Box::new(Expr::call("convert", vec![target, value], span)),
                        span,
                    },
                    span,
                }],
                span,
            },
            span,
        });
    }

    let is_parametric = !struct_def.type_params.is_empty();
    methods.push(SyntheticConstructorMethod {
        kind: SyntheticConstructorKind::DefaultInner(if is_parametric {
            ConstructorSelfFamily::ExplicitParametricInner
        } else {
            ConstructorSelfFamily::BareInner
        }),
        ctor: crate::ir::core::InnerConstructor {
            params: param_names
                .iter()
                .map(|name| crate::ir::core::TypedParam::new(name.clone(), None, span))
                .collect(),
            kwparams: Vec::new(),
            type_params: struct_def.type_params.clone(),
            is_explicit_parametric: is_parametric,
            explicit_type_parameter_names: struct_def
                .type_params
                .iter()
                .map(|param| param.name.clone())
                .collect(),
            explicit_type_arguments: struct_def
                .type_params
                .iter()
                .map(|param| TypeExpr::TypeVar(param.name.clone()))
                .collect(),
            body: Block {
                stmts: vec![Stmt::Return {
                    value: Some(Expr::New {
                        type_args: struct_def
                            .type_params
                            .iter()
                            .map(|param| TypeExpr::TypeVar(param.name.clone()))
                            .collect(),
                        args: converted_values,
                        is_splat: false,
                        span,
                    }),
                    span,
                }],
                span,
            },
            span,
        },
    });

    Ok(methods)
}

fn register_type_alias(
    shared_ctx: &mut SharedCompileContext,
    alias: &crate::ir::core::TypeAliasDef,
) {
    shared_ctx
        .type_aliases
        .insert(alias.name.clone(), type_alias_runtime_target(alias));
}

fn register_module_type_aliases(
    shared_ctx: &mut SharedCompileContext,
    module: &crate::ir::core::Module,
    prefix: &str,
) {
    let module_path = if prefix.is_empty() {
        module.name.clone()
    } else {
        format!("{}.{}", prefix, module.name)
    };

    for alias in &module.type_aliases {
        let target = type_alias_runtime_target(alias);
        shared_ctx
            .type_aliases
            .insert(format!("{}.{}", module_path, alias.name), target);
    }

    for submodule in &module.submodules {
        register_module_type_aliases(shared_ctx, submodule, &module_path);
    }
}

/// Collect every top-level `Stmt::Assign` reachable through `Stmt::Block`
/// nesting only (not `if`/`for`/function bodies). `const NAME = ...` lowers to
/// `wrap_const_assignment`'s `Stmt::Block([#__sjulia_declare_const__ call,
/// Stmt::Assign])`, so a shallow top-level scan alone misses every `const`
/// binding — which is exactly the shape `const MyPair = Pair` uses.
fn collect_block_nested_assigns<'a>(stmts: &'a [Stmt], out: &mut Vec<&'a Stmt>) {
    for stmt in stmts {
        match stmt {
            Stmt::Assign { .. } => out.push(stmt),
            Stmt::Block(block) => collect_block_nested_assigns(&block.stmts, out),
            _ => {}
        }
    }
}

/// Whether `name` denotes a real, already-known type at compile time — the
/// gate that decides whether a bare-identifier const binding is a TYPE alias
/// rather than an ordinary value binding (Issue #11113).
///
/// Two independent tables answer this, because no single one covers every
/// Base/stdlib type: `struct_table` knows every Pure-Julia struct declaration
/// (`Pair`, `VersionNumber`, ..., in BOTH cache modes — cold-compiled or
/// Base-cache-restored), while some Base type names (`Regex`, `SubString`,
/// `UnitRange`, ...) are native VM concepts with NO `struct_table` entry at
/// all and are instead recognized by the compiler-visible builtin-type
/// registry (`is_builtin_type_name`). Consulting both, instead of a
/// hand-maintained name list, is what makes this general: it is driven by
/// what the compiler actually knows to be a type, not by which names someone
/// remembered to enumerate.
fn resolves_to_known_type(name: &str, struct_table: &StructRegistry) -> bool {
    struct_table.resolve(name).is_some() || crate::compile::type_helpers::is_builtin_type_name(name)
}

/// Compile-time top-up for #11113: register `var -> rhs` in `type_aliases`
/// for every top-level `const var = rhs` / `var = rhs` binding whose RHS is a
/// bare identifier that resolves to an already-known type — i.e. the binding
/// aliases a TYPE, not an ordinary value. Chases through aliases registered
/// earlier in `type_aliases` (including by lowering's own `TypeAliasDef`s) so
/// `const B = MyPair; const MyPair = Pair` still lands on the ultimate type
/// name whichever order this scan sees them.
///
/// A binding whose RHS never resolves to a type (`const MAX = 5`, `const FOO
/// = SOME_UPPERCASE_CONSTANT`) is silently left alone: [`resolves_to_known_type`]
/// answers "is this name actually a type" structurally, so nothing here
/// depends on identifier casing or a maintained name list, and non-type
/// bindings behave exactly as before (Issue #11113, design principle #10).
fn register_struct_table_backed_aliases(
    type_aliases: &mut HashMap<String, String>,
    struct_table: &StructRegistry,
    stmts: &[Stmt],
) {
    let mut assigns = Vec::new();
    collect_block_nested_assigns(stmts, &mut assigns);
    // Bounded fixpoint: a chain (`const B = MyPair; const MyPair = Pair`)
    // registers regardless of which binding this scan reaches first.
    for _ in 0..8 {
        let mut changed = false;
        for stmt in &assigns {
            let Stmt::Assign { var, value, .. } = stmt else {
                continue;
            };
            if type_aliases.contains_key(var) {
                continue;
            }
            // A bare uppercase identifier that names a struct resolves to
            // `Expr::FunctionRef` (structs double as their own constructor
            // function), not `Expr::Var` — `const MyInt = Int64` reaches
            // `Expr::Var` because a builtin scalar name never binds a
            // function, but `const MyPair = Pair` does not (Issue #11113).
            let (Expr::Var(rhs, _) | Expr::FunctionRef { name: rhs, .. }) = value else {
                continue;
            };
            let rhs_name = rhs.as_ref();
            if rhs_name == var {
                continue;
            }
            let resolved_rhs = type_aliases
                .get(rhs_name)
                .cloned()
                .unwrap_or_else(|| rhs_name.to_string());
            if resolves_to_known_type(&resolved_rhs, struct_table) {
                type_aliases.insert(var.clone(), resolved_rhs);
                changed = true;
            }
        }
        if !changed {
            break;
        }
    }
}

/// Module-body variant of [`register_struct_table_backed_aliases`] (Issue
/// #11113), mirroring [`register_module_type_aliases`]'s qualified-name
/// convention so a module-local alias of a Base/stdlib type resolves the same
/// way a top-level one does.
fn register_struct_table_backed_module_aliases(
    type_aliases: &mut HashMap<String, String>,
    struct_table: &StructRegistry,
    module: &crate::ir::core::Module,
    prefix: &str,
) {
    let module_path = if prefix.is_empty() {
        module.name.clone()
    } else {
        format!("{}.{}", prefix, module.name)
    };
    let mut assigns = Vec::new();
    collect_block_nested_assigns(&module.body.stmts, &mut assigns);
    let qualified_stmts: Vec<(String, &Expr)> = assigns
        .into_iter()
        .filter_map(|stmt| match stmt {
            Stmt::Assign { var, value, .. } => Some((format!("{}.{}", module_path, var), value)),
            _ => None,
        })
        .collect();
    for _ in 0..8 {
        let mut changed = false;
        for (qualified_var, value) in &qualified_stmts {
            if type_aliases.contains_key(qualified_var) {
                continue;
            }
            let value: &Expr = value;
            let (Expr::Var(rhs, _) | Expr::FunctionRef { name: rhs, .. }) = value else {
                continue;
            };
            let rhs_name = rhs.as_ref();
            let qualified_rhs = format!("{}.{}", module_path, rhs_name);
            if qualified_rhs == *qualified_var {
                continue;
            }
            let resolved_rhs = type_aliases
                .get(&qualified_rhs)
                .or_else(|| type_aliases.get(rhs_name))
                .cloned()
                .unwrap_or_else(|| rhs_name.to_string());
            if resolves_to_known_type(&resolved_rhs, struct_table) {
                type_aliases.insert(qualified_var.clone(), resolved_rhs);
                changed = true;
            }
        }
        if !changed {
            break;
        }
    }
    for submodule in &module.submodules {
        register_struct_table_backed_module_aliases(
            type_aliases,
            struct_table,
            submodule,
            &module_path,
        );
    }
}

fn resolve_type_alias_in_module_scope(
    jt: JuliaType,
    module_path: Option<&String>,
    type_aliases: &HashMap<String, String>,
) -> JuliaType {
    fn alias_target(
        name: &str,
        module_path: Option<&String>,
        type_aliases: &HashMap<String, String>,
    ) -> Option<String> {
        if let Some(path) = module_path {
            if !name.contains('.') {
                let qualified = format!("{path}.{name}");
                if let Some(target) = type_aliases.get(&qualified) {
                    return Some(target.clone());
                }
            }
        }
        type_aliases.get(name).cloned()
    }

    fn resolve_name(
        name: &str,
        module_path: Option<&String>,
        type_aliases: &HashMap<String, String>,
    ) -> String {
        if let Some(target) = alias_target(name, module_path, type_aliases) {
            return target;
        }
        if let Some((base, params)) = parse_parametric_call(name) {
            if let Some(target) = alias_target(&base, module_path, type_aliases) {
                return format!("{target}{{{}}}", TypeExpr::render_param_list(&params));
            }
        }
        name.to_string()
    }

    match jt {
        JuliaType::Struct(name) => {
            JuliaType::from_name_or_struct(&resolve_name(&name, module_path, type_aliases))
        }
        JuliaType::VectorOf(inner) => JuliaType::VectorOf(Box::new(
            resolve_type_alias_in_module_scope(*inner, module_path, type_aliases),
        )),
        JuliaType::MatrixOf(inner) => JuliaType::MatrixOf(Box::new(
            resolve_type_alias_in_module_scope(*inner, module_path, type_aliases),
        )),
        JuliaType::TupleOf(types) => JuliaType::TupleOf(
            types
                .into_iter()
                .map(|ty| resolve_type_alias_in_module_scope(ty, module_path, type_aliases))
                .collect(),
        ),
        JuliaType::Union(types) => JuliaType::Union(
            types
                .into_iter()
                .map(|ty| resolve_type_alias_in_module_scope(ty, module_path, type_aliases))
                .collect(),
        ),
        JuliaType::TypeOf(inner) => JuliaType::TypeOf(Box::new(
            resolve_type_alias_in_module_scope(*inner, module_path, type_aliases),
        )),
        JuliaType::RuntimeParametric { base, params } => JuliaType::RuntimeParametric {
            base: resolve_name(&base, module_path, type_aliases),
            params: params
                .into_iter()
                .map(|ty| resolve_type_alias_in_module_scope(ty, module_path, type_aliases))
                .collect(),
        },
        JuliaType::RuntimeTypeVar {
            id,
            name,
            lower_bound,
            upper_bound,
        } => JuliaType::RuntimeTypeVar {
            id,
            name,
            lower_bound: Box::new(resolve_type_alias_in_module_scope(
                *lower_bound,
                module_path,
                type_aliases,
            )),
            upper_bound: Box::new(resolve_type_alias_in_module_scope(
                *upper_bound,
                module_path,
                type_aliases,
            )),
        },
        JuliaType::UnionAll {
            var,
            lower_bound,
            bound,
            body,
        } => JuliaType::UnionAll {
            var,
            lower_bound: lower_bound
                .map(|name| Box::new(resolve_name(&name, module_path, type_aliases))),
            bound: bound.map(|name| Box::new(resolve_name(&name, module_path, type_aliases))),
            body: Box::new(resolve_type_alias_in_module_scope(
                *body,
                module_path,
                type_aliases,
            )),
        },
        JuliaType::RuntimeUnionAll { var, body } => JuliaType::RuntimeUnionAll {
            var: Box::new(resolve_type_alias_in_module_scope(
                *var,
                module_path,
                type_aliases,
            )),
            body: Box::new(resolve_type_alias_in_module_scope(
                *body,
                module_path,
                type_aliases,
            )),
        },
        other => other,
    }
}

fn imported_type_alias_target(
    shared_ctx: &SharedCompileContext,
    canonical_source: &str,
) -> Option<String> {
    if let Some(target) = shared_ctx.type_aliases.get(canonical_source) {
        let Some((owner, _)) = canonical_source.rsplit_once('.') else {
            return Some(target.clone());
        };
        let target_head = target
            .split_once('{')
            .map_or(target.as_str(), |(head, _)| head);
        if target_head.contains('.') {
            return Some(target.clone());
        }
        let qualified_head = format!("{owner}.{target_head}");
        let owner_defines_target = shared_ctx.type_aliases.contains_key(&qualified_head)
            || shared_ctx.struct_table.contains_key(&qualified_head)
            || shared_ctx.parametric_structs.contains_key(&qualified_head)
            || shared_ctx
                .abstract_type_by_name
                .contains_key(&qualified_head)
            || shared_ctx.enum_types.contains_key(&qualified_head)
            || shared_ctx.is_primitive_type_name(&qualified_head);
        if owner_defines_target {
            return Some(format!("{owner}.{target}"));
        }
        return Some(target.clone());
    }

    if shared_ctx.struct_table.contains_key(canonical_source)
        || shared_ctx.parametric_structs.contains_key(canonical_source)
        || shared_ctx
            .abstract_type_by_name
            .contains_key(canonical_source)
        || shared_ctx.enum_types.contains_key(canonical_source)
        || shared_ctx.is_primitive_type_name(canonical_source)
    {
        return Some(canonical_source.to_string());
    }

    // Base/Core builtin types use their canonical unqualified Julia spelling
    // in type metadata (`Int`, `Type`, ...), even if an import source reaches
    // this phase as `Base.Int`/`Core.Type`. Normalize the namespace as a class,
    // then let JuliaType's builtin registry decide whether the leaf is a type;
    // never special-case individual builtin names.
    let builtin_spelling = canonical_source
        .strip_prefix("Base.")
        .or_else(|| canonical_source.strip_prefix("Core."))
        .filter(|name| !name.contains('.'))
        .unwrap_or(canonical_source);
    JuliaType::from_name(builtin_spelling).map(|_| builtin_spelling.to_string())
}

/// Whether an import binding hits upstream's warn-and-ignore conflict: the
/// destination module already binds the same name through a source-earlier
/// plain assignment. Emits the upstream warning once per conflicting import
/// (Issue #11426).
fn import_conflicts_with_existing_binding(
    shared_ctx: &SharedCompileContext,
    qualified_alias: &str,
    using_import: &UsingImport,
    canonical_source: &str,
    current_module_path: &str,
) -> bool {
    let Some(&binding_order) = shared_ctx
        .module_value_binding_positions
        .get(qualified_alias)
    else {
        return false;
    };
    if binding_order >= using_import.span.definition_order {
        return false;
    }
    let destination = if current_module_path.is_empty() {
        "Main"
    } else {
        current_module_path
    };
    use std::io::Write;
    let _ = writeln!(
        std::io::stderr(),
        "WARNING: import of {canonical_source} into {destination} conflicts with an existing identifier; ignored."
    );
    true
}

/// Record where each module-body value binding sits relative to the import
/// chronology, keyed by qualified name (`"Sink.A"`), for the conflict gate
/// above (Issue #11426).
fn record_scope_value_binding_positions(
    shared_ctx: &mut SharedCompileContext,
    body: &crate::ir::core::Block,
    scope_path: &str,
) {
    let mut import_marker_starts = HashSet::new();
    collect::collect_module_body_import_marker_starts(body, &mut import_marker_starts);
    let mut positions = HashMap::new();
    let mut last_using_order = 0u64;
    collect::collect_module_body_value_binding_positions(
        body,
        &mut last_using_order,
        &import_marker_starts,
        &mut positions,
    );
    for (name, order) in positions {
        let qualified = if scope_path.is_empty() {
            name
        } else {
            format!("{scope_path}.{name}")
        };
        shared_ctx
            .module_value_binding_positions
            .entry(qualified)
            .or_insert(order);
    }
}

fn register_scope_imported_type_aliases(
    shared_ctx: &mut SharedCompileContext,
    usings: &[UsingImport],
    current_module_path: &str,
    module_functions: &HashMap<String, HashSet<String>>,
) {
    // All explicit rename bindings participate in one source-ordered namespace,
    // regardless of whether the imported value is a module, function, type, or
    // ordinary value. Record non-type owners too so a later type alias cannot
    // overwrite the expression-level first winner (Issue #11176).
    let mut claimed_explicit_aliases = HashSet::new();
    for using_import in usings {
        let Some(resolved_module) =
            resolve_using_module_name(using_import, current_module_path, module_functions)
        else {
            continue;
        };
        for (source, alias) in &using_import.alias_bindings {
            let qualified_alias = if current_module_path.is_empty() {
                alias.clone()
            } else {
                format!("{current_module_path}.{alias}")
            };
            if !claimed_explicit_aliases.insert(qualified_alias.clone()) {
                continue;
            }
            let canonical_source =
                canonical_import_alias_source(using_import, &resolved_module, source);
            // Upstream ignores an import whose name is already bound by a
            // source-earlier module-level assignment (warn-and-ignore); the
            // existing binding stays authoritative for static type
            // resolution too (Issue #11426).
            if import_conflicts_with_existing_binding(
                shared_ctx,
                &qualified_alias,
                using_import,
                &canonical_source,
                current_module_path,
            ) {
                continue;
            }
            let Some(target) = imported_type_alias_target(shared_ctx, &canonical_source) else {
                continue;
            };
            // Module-local type aliases were pre-registered before imports.
            // Keep that local owner instead of letting an ignored conflicting
            // import rewrite static signature resolution (Issue #11176). The
            // import-before-definition activation/error boundary remains part
            // of the chronology work tracked by Issue #11097/#11131.
            if shared_ctx.type_aliases.contains_key(&qualified_alias) {
                continue;
            }
            shared_ctx
                .type_aliases
                .insert(qualified_alias.clone(), target);
            // Preserve the import's activation coordinate even though the
            // current alias resolver still pre-registers the whole scope.
            // Issue #11097/#11131 will gate eager signature evaluation on
            // this carrier; keeping it beside ordinary type-definition
            // positions avoids inventing a second chronology domain.
            let position = TypeDefinitionPosition {
                definition_order: using_import.span.definition_order,
                source_start: using_import.span.start,
            };
            shared_ctx
                .type_definition_positions
                .entry(qualified_alias)
                .and_modify(|existing| {
                    if position.is_before(existing.definition_order, existing.source_start) {
                        *existing = position;
                    }
                })
                .or_insert(position);
        }
        if let Some(symbols) = &using_import.symbols {
            for symbol in symbols {
                let qualified_symbol = if current_module_path.is_empty() {
                    symbol.clone()
                } else {
                    format!("{current_module_path}.{symbol}")
                };
                // A selective import hits the same warn-and-ignore conflict
                // as a rename when the name already has a source-earlier
                // module binding (Issue #11426).
                let canonical_source = format!("{resolved_module}.{symbol}");
                let _ = import_conflicts_with_existing_binding(
                    shared_ctx,
                    &qualified_symbol,
                    using_import,
                    &canonical_source,
                    current_module_path,
                );
                claimed_explicit_aliases.insert(qualified_symbol);
            }
        }
    }
}

fn register_module_imported_type_aliases(
    shared_ctx: &mut SharedCompileContext,
    module: &crate::ir::core::Module,
    prefix: &str,
    module_functions: &HashMap<String, HashSet<String>>,
) {
    let module_path = if prefix.is_empty() {
        module.name.clone()
    } else {
        format!("{prefix}.{}", module.name)
    };
    record_scope_value_binding_positions(shared_ctx, &module.body, &module_path);
    register_scope_imported_type_aliases(
        shared_ctx,
        &module.usings,
        &module_path,
        module_functions,
    );
    for submodule in &module.submodules {
        register_module_imported_type_aliases(
            shared_ctx,
            submodule,
            &module_path,
            module_functions,
        );
    }
}

fn module_imports_base_symbol(
    module_path: Option<&String>,
    module_usings_map: &HashMap<String, Vec<UsingImport>>,
    symbol: &str,
) -> bool {
    let Some(path) = module_path else {
        return false;
    };
    module_usings_map.get(path).is_some_and(|usings| {
        usings.iter().any(|using_import| {
            !using_import.is_relative
                && using_import.module == "Base"
                && using_import
                    .symbols
                    .as_ref()
                    .is_some_and(|symbols| symbols.iter().any(|imported| imported == symbol))
        })
    })
}

fn should_defer_module_return_inference(
    func: &Function,
    module_path: Option<&String>,
    is_base_function: bool,
) -> bool {
    if is_base_function || func.return_type.is_some() {
        return false;
    }
    let Some(path) = module_path else {
        return false;
    };
    path != "Core"
        && !path.starts_with("Core.")
        && path != "Base"
        && !path.starts_with("Base.")
        && path != "Main"
        && !path.starts_with("Main.")
}

fn resolve_inner_constructor_target(
    struct_def: &crate::ir::core::StructDef,
    qualified_struct_name: &str,
    struct_table: &StructRegistry,
    parametric_structs: &HashMap<String, ParametricStructDef>,
) -> CResult<InnerCtorTarget> {
    let (family, target) = if struct_def.is_parametric() {
        (
            "parametric",
            parametric_structs
                .contains_key(qualified_struct_name)
                .then(|| InnerCtorTarget::Parametric {
                    qualified_name: qualified_struct_name.to_string(),
                }),
        )
    } else {
        let owner_path = qualified_struct_name.rsplit_once('.').map_or(
            subset_julia_vm_bytecode::module_intern::MAIN_MODULE_PATH,
            |(owner, _)| owner,
        );
        (
            "concrete",
            struct_table
                .resolve_in_owner(owner_path, qualified_struct_name)
                .map(|(_struct_id, info)| InnerCtorTarget::Concrete {
                    type_id: info.type_id,
                }),
        )
    };
    target.ok_or_else(|| {
        internal_compile_error(format!(
            "missing {family} inner-constructor target `{qualified_struct_name}` at line {}, column {}",
            struct_def.span.start_line, struct_def.span.start_column
        ))
    })
}

/// Pipeline state threaded between the named phases of
/// `compile_core_program_internal` (Issue #6333). Borrowed fields point at the
/// merged/optimized source IR prepared by the source phases; owned fields are
/// the tables and bytecode accumulated by the build/compile phases and finally
/// consumed by [`CorePipeline::finalize`].
struct CorePipeline<'a> {
    // Source IR (merged Base + user, after inline/optimize passes)
    program: &'a Program,
    opt_user_functions: &'a [Function],
    opt_modules: &'a [crate::ir::core::Module],
    opt_main: &'a Block,
    /// Program modules chained with `using`-loaded stdlib modules.
    all_modules: Vec<&'a crate::ir::core::Module>,
    /// Inline (nested) functions with their parent function names.
    inline_functions: &'a [(Function, Option<String>)],
    /// Indices into `inline_functions` for lifted helpers that originated from
    /// Base/prelude main or Base/prelude function bodies. These helpers are
    /// part of the Base cache's compiled function-info prefix, but may be
    /// placed after user functions in this compile's `all_functions` order
    /// (Issue #10211).
    base_lifted_inline_indices: HashSet<usize>,
    /// `inline_functions` collection index -> owning module path, for entries
    /// found directly inside a module-body `let`/`@testset` (Issue #10073).
    /// Keyed by index (not bare name, Issue #10214/#10236) so that two
    /// different modules' (or a module's and Main's) same-named let/testset
    /// root helpers do not collide.
    module_scope_overrides: &'a HashMap<usize, String>,
    base_function_count: usize,
    // REPL session globals (resolved once struct tables are built)
    global_types: &'a HashMap<String, ValueType>,
    global_struct_names: &'a HashMap<String, String>,
    // Optional cache inputs (Issue #2933)
    precompiled_base: Option<&'a CompiledProgram>,
    cached_method_tables: Option<&'a HashMap<String, MethodTable>>,
    cached_closure_captures: Option<&'a HashMap<String, HashSet<String>>>,
    cached_inference_results: Option<&'a [(InferenceCacheKey, CachedReturn)]>,
    /// Preloaded-package bytecode cache (Issue #9189): module path -> that
    /// module's precompiled functions. `None` whenever the build-time
    /// `preload_cache::PRELOAD_PACKAGES` configuration is empty.
    preload_module_cache: Option<&'a HashMap<String, super::preload_cache::CachedPreloadModule>>,
    /// The preload cache's whole-closure non-Base function layout (Issue #9230).
    /// `build_method_tables` gates preload activation on this program's non-Base
    /// prefix starting with it (layout identity, no relocation).
    preload_closure_layout: Option<&'a [(Option<String>, String)]>,
    /// Extra Main-scope function names accessible via the reused prefix but not
    /// present in this compile's IR (REPL input-delta, Issue #9199 S5). Folded
    /// into `imported_functions` so a delta eval can call prior-defined
    /// functions. `None` for every non-delta compile.
    extra_imported_functions: Option<&'a HashSet<String>>,
    /// `function_infos` index -> the cache entry `build_method_tables` matched
    /// for it (Issue #9189). Populated there (right after its `params` local
    /// is resolved — the ONLY place both the bare IR name and the
    /// fully-qualified/resolved parameter types are simultaneously available
    /// without re-deriving them, see `preload_cache::signature_key_for_resolved_params`'s
    /// doc comment); consumed by `compile_functions` (skip codegen for this
    /// index) and `finalize` (splice the cached body in after both peephole
    /// passes, since — unlike the true cached-Base prefix — a module
    /// function's position in the code buffer varies per run and can't be
    /// folded into `reused_base`'s existing flat-prefix-shift handling).
    preload_reused: HashMap<usize, super::preload_cache::CachedPreloadFunction>,
    // Type definitions
    all_structs: Vec<StructOriginEntry<'a>>,
    runtime_inner_constructor_keys: HashSet<(String, u64, usize)>,
    module_struct_names: HashMap<String, HashSet<String>>,
    abstract_types: Vec<AbstractTypeDefInfo>,
    abstract_type_names: HashSet<String>,
    abstract_type_parents: HashMap<String, Option<String>>,
    primitive_types: Vec<PrimitiveTypeDefInfo>,
    shared_ctx: SharedCompileContext,
    // Pending REPL globals, resolved after struct_table is built
    pending_global_types: HashMap<String, ValueType>,
    pending_global_struct_names: HashMap<String, String>,
    // Method tables and function metadata
    method_tables: HashMap<String, MethodTable>,
    /// Rc-shared with the Base cache for the cached prefix (Issue #9140):
    /// entries `0..cached_base_len` alias the cache's `FunctionInfo`s and are
    /// never mutated (guarded by `reused_base`); user entries are freshly
    /// created (refcount 1), so `Rc::make_mut` mutates them in place.
    function_infos: Vec<std::rc::Rc<FunctionInfo>>,
    global_index: usize,
    cached_base_len: usize,
    /// Maps all_functions index -> function_infos index.
    func_index_map: Vec<usize>,
    show_methods: Vec<ShowMethodEntry>,
    print_methods: Vec<ShowMethodEntry>,
    /// Lazy AoT: functions that need specialization.
    specializable_functions: Vec<SpecializableFunction>,
    // Module metadata
    /// Module-path <-> `ModuleId` interning table (Issue #10988 Phase 2a),
    /// populated in `collect_module_metadata` by walking `all_modules` in
    /// depth-first, source-declared order (`register_module_ids`) — the
    /// SAME recursion `collect_module_info` uses to build `module_functions`
    /// below, so every module path registered here is also a
    /// `module_functions` key. Consulted (never re-derived from `HashMap`
    /// order) when `finalize` builds `macro_bindings`/`RuntimeCompileContext`.
    module_registry: ModuleInternTable,
    module_functions: HashMap<String, HashSet<String>>,
    module_exports: HashMap<String, HashSet<String>>,
    /// Module-level constants (variables assigned in module body).
    module_constants: HashMap<String, HashSet<String>>,
    imported_functions: HashSet<String>,
    usings_set: HashSet<String>,
    module_imports_map: HashMap<String, HashSet<String>>,
    module_usings_map: HashMap<String, Vec<UsingImport>>,
    /// Top-level selective-import name -> source module(s)
    /// (`import M: f` / `using M: f`). A later top-level `function f(...)` extends
    /// `M.f`, so its method must also join the `M.f` table — not just `f`
    /// (Issue #8052).
    toplevel_import_sources: HashMap<String, Vec<String>>,
    // Function universe (Base + module + user + inline functions)
    base_function_names: HashSet<String>,
    user_function_names: HashSet<String>,
    all_functions: Vec<(&'a Function, Option<String>)>,
    first_user_function_idx: usize,
    inline_start_idx: usize,
    repl_current_function_count: Option<usize>,
    repl_current_struct_count: Option<usize>,
    repl_append_only_new_generics: bool,
    func_idx_to_parent: HashMap<usize, String>,
    /// `all_functions` indices of functions collected directly from a
    /// module-body `let`/`@testset` (via `module_scope_overrides`), as
    /// opposed to a genuine top-level `function`/`f(...) = ...` declared in
    /// `module.functions`. Such a function is a LEXICALLY-SCOPED LOCAL of its
    /// defining `let`/`@testset` block, not a module-level generic function:
    /// its RUNTIME `function_name_index` bare (short-name) alias must be
    /// suppressed. That name index — used by `Value::Closure`/`Value::Function`
    /// dynamic dispatch, a separate index from `method_tables` — is shared
    /// across every module (and Main), so two different modules' (or a module's
    /// and Main's) same-named `let`-root helper would dedup-collide into a
    /// single bare name-index entry, silently routing one scope's call to the
    /// OTHER scope's body (Issue #10236). The method-table registration itself
    /// is unchanged: such a function still joins BOTH the bare and the
    /// module-qualified table (needed for `module_owned_function_table_name`'s
    /// own-module redirect, Issue #7575). `build_method_tables` consults this
    /// set to drive `FunctionInfo::suppress_short_name_alias`.
    module_body_scoped_root_indices: HashSet<usize>,
    /// Final `all_functions` indices for cache-covered Base/prelude functions
    /// that are not in the flat `0..base_function_count` top-level prefix:
    /// loaded Base/stdlib module functions plus lifted Base helpers
    /// (Issue #10211).
    base_cached_extra_function_indices: HashSet<usize>,
    cached_base_extra_reused_count: usize,
    callable_typeof_aliases: HashMap<String, String>,
    // Inference bookkeeping
    has_seeded_inference_results: bool,
    shadowed_user_globals: HashSet<String>,
    has_opaque_runtime_eval: bool,
    opaque_runtime_eval_function_names: HashSet<String>,
    current_input_type_names: Option<HashSet<String>>,
    pre_optimization_runtime_nominal_names: HashSet<String>,
    // Code generation state
    inner_ctors: Vec<InnerCtorInfo>,
    code: Vec<Instr>,
    source_map: Vec<Option<crate::span::Span>>,
    reused_base: Vec<bool>,
    base_main_entry: Option<usize>,
    /// Absolute code offset where the USER main block begins (Issue #9199 LV2),
    /// captured in `compile_main` and carried through the peephole passes in
    /// `finalize`. Distinct from `entry` (which points at the base-main prefix so
    /// Base initializers run first): this is where the user's own top-level code
    /// starts, the slice boundary for the REPL live-append relocatable delta main.
    user_main_entry: Option<usize>,
    /// Live VM frame-0 global-slot names to seed the main block's global-slot
    /// assignment, from `CompilerCacheInput::global_slot_seed` (Issue #9199 LV2).
    global_slot_seed: Option<&'a [String]>,
    /// Prior modules' surface (function/constant/export names) folded into
    /// `module_functions` / `module_exports` / `module_constants` so a REPL
    /// relocatable-delta compile can resolve a reference to a prior module
    /// (`M.f()`, `M.const`) whose body is NOT in this delta's IR (Issue #9199
    /// LV5). `None` for every non-delta / module-free compile.
    extra_module_metadata: Option<&'a crate::compile::cache::ReplModuleMetadata>,
    extra_inner_constructor_type_names: Option<&'a HashSet<String>>,
    deferred_shadowed_global_types: Vec<(String, Option<ValueType>)>,
    modules_entry: usize,
    entry: usize,
    /// Names still genuinely bound at module/main scope when `compile_main`
    /// finishes — i.e. `main_compiler.initialized_locals` at the end of the
    /// main compile. A `let`/`@testset` block fully restores
    /// `initialized_locals` to its pre-block snapshot when it exits (see
    /// `Expr::LetBlock` compilation), so a name introduced only inside such a
    /// hard-scope block is absent here even though its slot still exists in
    /// the compiled main bytecode. Used to scope the peephole's main-store
    /// protection (Issue #9157) to real main-scope bindings only, so a
    /// `let`-local's store remains eligible for the standard
    /// store-elimination optimization and does not leak into REPL session
    /// persistence (`Vm::get_global`).
    main_scope_names: HashSet<String>,
}

/// Coarse compile-cost classification of a program (Issue #10127).
///
/// `program` (as received by `compile_core_program_internal`) has already
/// been through `parse_and_lower`'s prelude merge: `program.structs`/
/// `program.abstract_types`/`program.usings`/`program.main.stmts` all
/// unconditionally contain the prelude's own ~130+ structs, ~30+ abstract
/// types, usings, and top-level statements FIRST, with the user's own
/// content appended after (`program.functions` is the one field with an
/// explicit split: `base_function_count`). Comparing lengths against
/// `crate::get_prelude_program()` — the same already-parsed, process-cached
/// `Program` `parse_and_lower` merged in, so this is a cheap length check,
/// not a re-parse — gives an exact "did the user add anything" test for
/// structs/abstract-types/usings without needing to reason about which
/// individual entries are "Base's own": the prelude's own struct/abstract
/// list for a given sjulia build is fixed, so any TOTAL count beyond it is
/// necessarily user-added. (An earlier version of this classifier tried to
/// name-match against the persisted Base cache's `struct_defs`, but that
/// cache only contains INSTANTIATED concrete parametric structs — e.g.
/// `Rational{Int64}` — never the generic template name `Rational` prelude
/// itself declares, so every compile misclassified as `FullPipeline`.) The
/// user-visible main statements are whatever follows the
/// `BASE_USER_MAIN_BOUNDARY_META` marker `parse_and_lower` inserts (absent
/// only for a from-scratch/no-prelude compile, where the whole thing is
/// user-visible).
///
/// This is currently a diagnostic signal (recorded via `profile::note` as
/// `compile.program_complexity` — visible under `SJULIA_COMPILE_PROFILE=1`)
/// rather than a hard gate on `build_struct_tables`/`build_inference_engine`/
/// `preinstantiate_parametric_types`/`collect_module_metadata`: profiling
/// `println("Hello World")` (classified `Trivial`) shows all four already at
/// or below ~0.3ms each on the cached-Base path — `build_struct_tables`
/// (~0.05ms) and `collect_module_metadata` (~0.3ms, dominated by populating
/// `imported_functions` from the ~5000 Base/prelude names, a set every
/// downstream call-site validity check reads — see the doc comment on
/// `collect_module_metadata`'s dedup fix, Issue #10130) already skip Base's
/// own content when a cache is present, `preinstantiate_parametric_types`
/// already skips the whole Base/prelude prefix, and `build_inference_engine`
/// dropped from ~2ms to ~0.15ms via the Issue #10114 prefetch fix landed
/// alongside this one. Adding a parallel no-op code path for phases that are
/// already this cheap would mostly duplicate logic for a sub-millisecond
/// combined return, at real risk to the pervasive `imported_functions`
/// call-site-validity contract — so this type is kept as a tested,
/// general-purpose classifier future gating work (here or in a follow-up
/// Issue) can build on, without forcing a synthetic gate today.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum ProgramComplexity {
    /// No custom structs/using/abstract types/function definitions, and the
    /// user-visible main body is a single simple call.
    Trivial,
    /// No custom structs/using/abstract types/function definitions, but the
    /// user-visible main body has more than one statement (e.g. variable
    /// assignments).
    Simple,
    /// Declares at least one custom struct/using/abstract type/function —
    /// the full pipeline phases may have real work to do.
    FullPipeline,
}

impl ProgramComplexity {
    fn label(self) -> &'static str {
        match self {
            ProgramComplexity::Trivial => "trivial",
            ProgramComplexity::Simple => "simple",
            ProgramComplexity::FullPipeline => "full_pipeline",
        }
    }
}

/// True when `expr` is a call whose arguments are all literals (no
/// parameter/global references, kwargs, or splats) — the same "nothing to
/// resolve beyond constant folding" shape `is_trivial_ssa_fast_path_body`
/// (Issue #10115) uses for function bodies, reused here for the top-level
/// main statement.
fn is_simple_literal_call(expr: &Expr) -> bool {
    matches!(
        expr,
        Expr::Call {
            kwargs,
            splat_mask,
            kwargs_splat_mask,
            args,
            ..
        } if kwargs.is_empty()
            && kwargs_splat_mask.iter().all(|splat| !splat)
            && splat_mask.iter().all(|splat| !splat)
            && args.iter().all(|arg| matches!(arg, Expr::Literal(..)))
    )
}

/// Classifies `program` — see [`ProgramComplexity`]'s doc comment.
pub(crate) fn classify_program_complexity(program: &Program) -> ProgramComplexity {
    let prelude = crate::get_prelude_program();
    let has_custom_functions = program.functions.len() > program.base_function_count;
    let has_custom_types = match prelude {
        Some(prelude) => {
            program.structs.len() > prelude.structs.len()
                || program.abstract_types.len() > prelude.abstract_types.len()
        }
        None => !program.structs.is_empty() || !program.abstract_types.is_empty(),
    };
    let has_usings = match prelude {
        Some(prelude) => program.usings.len() > prelude.usings.len(),
        None => !program.usings.is_empty(),
    };
    if has_custom_functions || has_custom_types || has_usings {
        return ProgramComplexity::FullPipeline;
    }

    let user_stmts = match program
        .main
        .stmts
        .iter()
        .position(is_base_user_main_boundary)
    {
        Some(idx) => &program.main.stmts[idx + 1..],
        None => program.main.stmts.as_slice(),
    };
    let is_trivial_tail =
        |expr: &Expr| matches!(expr, Expr::Literal(..)) || is_simple_literal_call(expr);
    match user_stmts {
        [Stmt::Expr { expr, .. }] if is_trivial_tail(expr) => ProgramComplexity::Trivial,
        [Stmt::Return {
            value: Some(expr), ..
        }] if is_trivial_tail(expr) => ProgramComplexity::Trivial,
        _ => ProgramComplexity::Simple,
    }
}

/// Internal compilation with optional precompiled Base cache and method tables.
/// Returns a [`CoreCompileOutput`] carrying the compiled program plus the
/// method tables, closure captures, and inference results for caching.
pub(crate) fn compile_core_program_internal(
    program: &Program,
    global_types: &HashMap<String, ValueType>,
    global_struct_names: &HashMap<String, String>,
    cache_input: CompilerCacheInput<'_>,
) -> CResult<CoreCompileOutput> {
    let explicit_current_input_type_names = cache_input.current_input_type_names.cloned();
    let explicit_current_input_runtime_nominal_names =
        cache_input.current_input_runtime_nominal_names.cloned();
    // Issue #10127: diagnostic-only classification of the user's raw,
    // pre-merge_precompiled_base input — see `ProgramComplexity`'s doc
    // comment for why this is not (yet) wired into a hard gate on any phase
    // below.
    profile::note("compile.program_complexity", || {
        classify_program_complexity(program).label().to_string()
    });
    let (program_ref, base_function_count) = merge_precompiled_base(program);
    let opaque_runtime_eval =
        user_segment_opaque_runtime_eval(program_ref.as_ref(), base_function_count);
    let has_opaque_runtime_eval = opaque_runtime_eval.has_opaque_runtime_eval;
    let mut pre_optimization_runtime_nominal_names =
        explicit_current_input_runtime_nominal_names.unwrap_or_default();
    // Preserve runtime-conditional nominal names from the raw current input.
    // The merged Base/prelude/package prefix has its own installed bindings and
    // must not be mistaken for a source-ordered declaration in this eval.
    if cache_input.current_input_runtime_nominal_names.is_none() {
        let current_main_stmts = program
            .main
            .stmts
            .iter()
            .position(is_base_user_main_boundary)
            .map_or(program.main.stmts.as_slice(), |boundary| {
                &program.main.stmts[boundary + 1..]
            });
        pre_optimization_runtime_nominal_names.extend(collect_runtime_nominal_names_in_statements(
            current_main_stmts,
        ));
        for module in &program.modules {
            super::collect::collect_module_runtime_nominal_names(
                module,
                "",
                &mut pre_optimization_runtime_nominal_names,
            );
        }
    }
    let (inlined_program, optimized_user_segment) = inline_and_optimize_ir(
        program_ref.as_ref(),
        base_function_count,
        has_opaque_runtime_eval,
        cache_input.repl_current_function_count,
    );
    // The user-only optimization pass rewrites just user functions, modules,
    // and main; everything else (Base function prefix, structs, abstract
    // types, usings, ...) is read from the unmodified input program so the
    // Base IR is never deep-cloned per run (Issue #6348).
    let program = inlined_program.as_ref();
    let opt_user_functions: &Vec<Function> = &optimized_user_segment.user_functions;
    let opt_modules: &Vec<crate::ir::core::Module> = &optimized_user_segment.modules;
    let opt_main: &Block = &optimized_user_segment.main;

    let loaded_modules = load_stdlib_modules(program, opt_modules);

    // Combine program modules with loaded stdlib modules
    let all_modules: Vec<&crate::ir::core::Module> =
        opt_modules.iter().chain(loaded_modules.iter()).collect();

    // Collect inline functions from top-level statements (with parent function tracking)
    // inline_functions: Vec<(Function, Option<parent_func_name>)>
    // Keyed by `inline_functions` collection index, not bare name (Issue #10214/#10236).
    let mut module_scope_overrides: HashMap<usize, String> = HashMap::new();
    let mut base_lifted_inline_indices: HashSet<usize> = HashSet::new();
    let inline_functions: Vec<(Function, Option<String>)> = collect_top_level_inline_functions(
        program,
        base_function_count,
        opt_user_functions,
        opt_main,
        &all_modules,
        &mut module_scope_overrides,
        &mut base_lifted_inline_indices,
    );

    let mut p = CorePipeline::new(
        program,
        opt_user_functions,
        opt_modules,
        opt_main,
        all_modules,
        &inline_functions,
        &module_scope_overrides,
        base_lifted_inline_indices,
        base_function_count,
        global_types,
        global_struct_names,
        cache_input,
        has_opaque_runtime_eval,
        opaque_runtime_eval.function_names,
        explicit_current_input_type_names,
        pre_optimization_runtime_nominal_names,
    );

    let struct_tables = p.build_struct_tables();
    p.init_shared_context(struct_tables);
    p.seed_outputs_from_cache();

    let method_table_setup_timer = profile::start("compile.method_table_setup");
    profile::time("compile.collect_module_metadata", || {
        p.collect_module_metadata()
    });
    p.register_imported_type_aliases();
    p.validate_using_imports()?;
    profile::time("compile.build_function_universe", || {
        p.build_function_universe()
    });
    profile::time("compile.prepopulate_closure_captures", || {
        p.prepopulate_closure_captures()
    });
    profile::time("compile.preinstantiate_parametric_types", || {
        p.preinstantiate_parametric_types()
    });
    profile::time("compile.resolve_global_types", || p.resolve_global_types());
    profile::time("compile.resolve_module_imports", || {
        p.resolve_module_imports()
    });
    let mut inference_engine = p.build_inference_engine();
    profile::time("compile.build_method_tables", || {
        p.build_method_tables(&mut inference_engine)
    });
    profile::finish(method_table_setup_timer);

    p.register_inner_constructors(&mut inference_engine)?;
    p.project_method_table_hierarchy();
    p.analyze_module_lambda_captures();

    p.compile_functions()?;
    p.compile_inner_constructors()?;
    p.compile_base_main_prefix()?;
    p.compile_modules()?;
    p.compile_main()?;
    p.finalize(&inference_engine)
}

/// Collect the `Function.span.start` of every `Stmt::FunctionDef` nested
/// inside a construct whose body is not guaranteed to execute even when the
/// enclosing statement is reached (`if`/`while`/`for`/`try`: taken zero or
/// more times depending on a runtime condition) — as opposed to a
/// `let`/`@testset`/plain block, which always runs its body exactly once
/// when reached. Recurses through nested containers, including further
/// conditionals/loops, but never into a named `Function`'s own body: that
/// body is compiled and (de)activated entirely separately from
/// `user_main_stmts`'s own chronology.
///
/// `top_level_definition_activations`'s eager drain (Issues #9784/#11477/
/// #11118) assumes every collected top-level/inline function definition is
/// reached unconditionally by the point source-position order says so, and
/// is therefore safe to activate (`Instr::DefineEvalFunction`) purely by
/// comparing source positions against the next statement. That assumption is
/// false for a definition inside an untaken conditional branch or a
/// zero-iteration loop: `compile_stmt`'s own `Stmt::FunctionDef` handling
/// already emits the SAME `DefineEvalFunction` at its natural, correctly
/// branch-gated bytecode position, so the drain must never also activate it
/// — doing so unconditionally leaked the method into the runtime name table
/// regardless of whether its own statement ever executed, bypassing the
/// same runtime source-order visibility direct calls already honor via
/// `Vm::direct_function_visible_or_raise` (Issue #11320; siblings
/// #11286/#10461 track centralizing this single-authority visibility
/// decision project-wide).
fn collect_conditionally_gated_function_starts(
    stmts: &[Stmt],
    gated: bool,
    out: &mut HashSet<usize>,
) {
    for stmt in stmts {
        match stmt {
            Stmt::FunctionDef { func, .. } if gated => {
                out.insert(func.span.start);
            }
            Stmt::FunctionDef { .. } => {}
            Stmt::If {
                then_branch,
                else_branch,
                ..
            } => {
                collect_conditionally_gated_function_starts(&then_branch.stmts, true, out);
                if let Some(else_branch) = else_branch {
                    collect_conditionally_gated_function_starts(&else_branch.stmts, true, out);
                }
            }
            Stmt::While { body, .. }
            | Stmt::For { body, .. }
            | Stmt::ForEach { body, .. }
            | Stmt::ForEachTuple { body, .. } => {
                collect_conditionally_gated_function_starts(&body.stmts, true, out);
            }
            Stmt::Try {
                try_block,
                catch_block,
                else_block,
                finally_block,
                ..
            } => {
                collect_conditionally_gated_function_starts(&try_block.stmts, true, out);
                if let Some(catch_block) = catch_block {
                    collect_conditionally_gated_function_starts(&catch_block.stmts, true, out);
                }
                if let Some(else_block) = else_block {
                    collect_conditionally_gated_function_starts(&else_block.stmts, true, out);
                }
                if let Some(finally_block) = finally_block {
                    collect_conditionally_gated_function_starts(&finally_block.stmts, true, out);
                }
            }
            Stmt::Block(block) => {
                collect_conditionally_gated_function_starts(&block.stmts, gated, out);
            }
            Stmt::TestSet { body, .. } | Stmt::Timed { body, .. } => {
                collect_conditionally_gated_function_starts(&body.stmts, gated, out);
            }
            _ => {}
        }
    }
}

impl<'a> CorePipeline<'a> {
    fn toplevel_module_bindings(&self) -> HashSet<String> {
        self.opt_modules
            .iter()
            .map(|module| module.name.clone())
            .chain(
                self.extra_module_metadata
                    .into_iter()
                    .flat_map(|meta| meta.toplevel_module_bindings.iter().cloned()),
            )
            .collect()
    }

    #[allow(clippy::too_many_arguments)]
    fn new(
        program: &'a Program,
        opt_user_functions: &'a [Function],
        opt_modules: &'a [crate::ir::core::Module],
        opt_main: &'a Block,
        all_modules: Vec<&'a crate::ir::core::Module>,
        inline_functions: &'a [(Function, Option<String>)],
        module_scope_overrides: &'a HashMap<usize, String>,
        base_lifted_inline_indices: HashSet<usize>,
        base_function_count: usize,
        global_types: &'a HashMap<String, ValueType>,
        global_struct_names: &'a HashMap<String, String>,
        cache_input: CompilerCacheInput<'a>,
        has_opaque_runtime_eval: bool,
        opaque_runtime_eval_function_names: HashSet<String>,
        current_input_type_names: Option<HashSet<String>>,
        pre_optimization_runtime_nominal_names: HashSet<String>,
    ) -> Self {
        let CompilerCacheInput {
            precompiled_base,
            method_tables: cached_method_tables,
            closure_captures: cached_closure_captures,
            inference_results: cached_inference_results,
            preload_cache: preload_module_cache,
            preload_closure_layout,
            extra_imported_functions,
            global_slot_seed,
            extra_module_metadata,
            extra_inner_constructor_type_names,
            repl_current_function_count,
            repl_current_struct_count,
            repl_append_only_new_generics,
            current_input_type_names: _,
            current_input_runtime_nominal_names: _,
        } = cache_input;

        CorePipeline {
            program,
            opt_user_functions,
            opt_modules,
            opt_main,
            all_modules,
            inline_functions,
            base_lifted_inline_indices,
            module_scope_overrides,
            base_function_count,
            global_types,
            global_struct_names,
            precompiled_base,
            cached_method_tables,
            cached_closure_captures,
            cached_inference_results,
            preload_module_cache,
            preload_closure_layout,
            extra_imported_functions,
            preload_reused: HashMap::new(),
            all_structs: Vec::new(),
            runtime_inner_constructor_keys: HashSet::new(),
            module_struct_names: HashMap::new(),
            abstract_types: Vec::new(),
            abstract_type_names: HashSet::new(),
            abstract_type_parents: HashMap::new(),
            primitive_types: Vec::new(),
            shared_ctx: SharedCompileContext::new(
                StructRegistry::new(),
                Vec::new(),
                HashMap::new(),
                HashMap::new(),
                Vec::new(),
                0,
            ),
            pending_global_types: HashMap::new(),
            pending_global_struct_names: HashMap::new(),
            method_tables: HashMap::new(),
            function_infos: Vec::new(),
            global_index: 0,
            cached_base_len: 0,
            func_index_map: Vec::new(),
            show_methods: Vec::new(),
            print_methods: Vec::new(),
            specializable_functions: Vec::new(),
            module_registry: ModuleInternTable::new(),
            module_functions: HashMap::new(),
            module_exports: HashMap::new(),
            module_constants: HashMap::new(),
            imported_functions: HashSet::new(),
            usings_set: HashSet::new(),
            module_imports_map: HashMap::new(),
            module_usings_map: HashMap::new(),
            toplevel_import_sources: HashMap::new(),
            base_function_names: HashSet::new(),
            user_function_names: HashSet::new(),
            all_functions: Vec::new(),
            first_user_function_idx: 0,
            inline_start_idx: 0,
            repl_current_function_count,
            repl_current_struct_count,
            repl_append_only_new_generics,
            func_idx_to_parent: HashMap::new(),
            module_body_scoped_root_indices: HashSet::new(),
            base_cached_extra_function_indices: HashSet::new(),
            cached_base_extra_reused_count: 0,
            callable_typeof_aliases: HashMap::new(),
            has_seeded_inference_results: false,
            shadowed_user_globals: HashSet::new(),
            has_opaque_runtime_eval,
            opaque_runtime_eval_function_names,
            current_input_type_names,
            pre_optimization_runtime_nominal_names,
            inner_ctors: Vec::new(),
            code: Vec::new(),
            source_map: Vec::new(),
            reused_base: Vec::new(),
            base_main_entry: None,
            user_main_entry: None,
            global_slot_seed,
            extra_module_metadata,
            extra_inner_constructor_type_names,
            deferred_shadowed_global_types: Vec::new(),
            modules_entry: 0,
            entry: 0,
            main_scope_names: HashSet::new(),
        }
    }

    fn build_struct_tables(&mut self) -> StructTables {
        let program = self.program;
        let opt_modules = self.opt_modules;
        let precompiled_base = self.precompiled_base;
        let all_modules = &self.all_modules;

        // Build struct table from struct definitions
        // Separate parametric structs from concrete structs
        //
        // Issue #11078: the registry's owner ids must come from a module table
        // seeded by the SAME deterministic depth-first module walk the rest of
        // the pipeline uses (`register_module_ids`), not minted lazily in
        // struct-registration order — otherwise a struct's `StructId` would
        // depend on which lane registered it first, and the cached lane (which
        // seeds every cached `struct_defs` entry up front) would disagree with
        // the fresh lane. `collect_module_metadata` builds the identical table
        // later for `CorePipeline::module_registry`; both walks are pinned to
        // agree by `struct_registry_owner_ids_agree_with_pipeline_module_registry_11078`.
        let mut struct_module_registry = ModuleInternTable::new();
        for module in all_modules {
            register_module_ids(module, "", &mut struct_module_registry);
        }
        let mut struct_table: StructRegistry = StructRegistry::with_modules(struct_module_registry);
        let mut parametric_structs: HashMap<String, ParametricStructDef> = HashMap::new();
        let mut base_parametric_structs: HashMap<String, ParametricStructDef> = HashMap::new();

        // When using cache, initialize struct_defs from cached base to maintain consistent type_ids.
        // This is critical because cached bytecode contains NewStruct instructions with type_ids
        // that must match the struct_defs indices.
        //
        // Also build instantiation_table for parametric instantiations like Complex{Float64}
        // to prevent re-instantiation with different type_ids.
        let mut cached_instantiation_table: HashMap<InstantiationKey, usize> = HashMap::new();
        let (mut struct_defs, mut next_type_id): (Vec<StructDefInfo>, usize) =
            profile::time("compile.cached_struct_defs_init", || {
                if let Some(base_cache) = precompiled_base {
                    let cached_len = base_cache.struct_defs.len();
                    // Cached `StructDefInfo` does not carry the
                    // inner-constructor flag, so recover it from the merged
                    // program's IR (which includes the Base source structs).
                    // Rebuilding the entries with `has_inner_constructor:
                    // false` made the compiler synthesize the field-count
                    // default constructor for Base structs that suppress it —
                    // e.g. `WeakRef(x)` was compiled as a raw struct build
                    // instead of dispatching to the outer constructor
                    // `WeakRef(x) = _weakref_new(x)`, so the weak cell was
                    // never registered with the GC and `GC.gc()` could not
                    // clear it (Issue #10092).
                    let mut inner_ctor_flags = collect_inner_constructor_flags(
                        program.structs.iter(),
                        all_modules.iter().copied(),
                    );
                    if let Some(extra_names) = self.extra_inner_constructor_type_names {
                        inner_ctor_flags
                            .extend(extra_names.iter().cloned().map(|name| (name, true)));
                    }
                    // Also rebuild struct_table for cached structs so we can look them up
                    for (idx, def) in base_cache.struct_defs.iter().enumerate() {
                        struct_table.insert(
                            def.name.clone(),
                            StructInfo {
                                type_id: idx,
                                is_mutable: def.is_mutable,
                                fields: def.fields.clone(),
                                has_inner_constructor: inner_constructor_flag_for(
                                    &inner_ctor_flags,
                                    &def.name,
                                ),
                            },
                        );
                        // For parametric instantiations like "Complex{Float64}", build instantiation_table entry
                        if let Some(brace_idx) = def.name.find('{') {
                            let base_name = def.name[..brace_idx].to_string();
                            let type_args_str = &def.name[brace_idx + 1..def.name.len() - 1];
                            let Some(type_args) = parse_type_args_recursive(type_args_str) else {
                                continue;
                            };
                            let key = InstantiationKey {
                                base_name,
                                type_args,
                            };
                            cached_instantiation_table.insert(key, idx);
                        }
                    }
                    (base_cache.struct_defs.clone(), cached_len)
                } else {
                    (Vec::new(), 0)
                }
            });

        // Collect all structs: top-level (None) + module structs (Some(module_path)).
        // The third tuple item records whether bare struct-name resolution should
        // prefer the preserved Base-origin table when a later module alias
        // clobbered the live table (Issue #10383). Functions have
        // `base_function_count`; structs do not, so derive the equivalent origin
        // signal from merge order and module provenance:
        //
        // - top-level Base/prelude structs are prepended before root-source user
        //   structs by `merge_with_precompiled_base`;
        // - user modules are the `opt_modules` prefix of `all_modules`;
        // - stdlib/Base-loaded modules are appended after that prefix.
        let root_main_span = program.main.span;
        let merged_base_prefix_present = self.base_function_count > 0;
        let is_root_source_struct = |def: &crate::ir::core::StructDef| {
            def.span.start >= root_main_span.start
                && def.span.end <= root_main_span.end
                && def.span.start_line >= root_main_span.start_line
                && def.span.end_line <= root_main_span.end_line
        };
        let mut all_structs: Vec<StructOriginEntry<'_>> = program
            .structs
            .iter()
            .map(|s| {
                (
                    s,
                    None,
                    merged_base_prefix_present && !is_root_source_struct(s),
                )
            })
            .collect();

        let user_module_count = opt_modules.len();
        for (module_index, module) in all_modules.iter().enumerate() {
            let mut module_structs = Vec::new();
            collect_module_structs(module, "", &mut module_structs);
            let prefer_base_origin = module.is_base_origin || module_index >= user_module_count;
            for (struct_def, module_path) in module_structs {
                all_structs.push((struct_def, Some(module_path), prefer_base_origin));
            }
        }

        // A control-flow-owned struct is deliberately absent from
        // `Program::structs`: its declaration must not become visible before
        // execution reaches `DefineRuntimeNominal`. An explicit inner
        // constructor nevertheless has to be compiled ahead of time because
        // `new(...)` embeds the concrete allocation type_id in bytecode. Add
        // only those declarations to the private compiler type universe; the
        // VM hides the resulting row until the runtime marker commits it
        // (Issue #11679).
        let mut runtime_inner_structs = Vec::new();
        collect_runtime_inner_constructor_structs_in_block(
            self.opt_main,
            None,
            &mut runtime_inner_structs,
        );
        for module in all_modules {
            collect_module_runtime_inner_constructor_structs(
                module,
                "",
                &mut runtime_inner_structs,
            );
        }
        let mut known_declarations = all_structs
            .iter()
            .map(|(definition, module_path, _)| {
                (
                    module_path.as_ref().map_or_else(
                        || definition.name.clone(),
                        |path| format!("{path}.{}", definition.name),
                    ),
                    definition.span.definition_order,
                    definition.span.start,
                )
            })
            .collect::<HashSet<_>>();
        for (definition, module_path) in runtime_inner_structs {
            let qualified_name = module_path.as_ref().map_or_else(
                || definition.name.clone(),
                |path| format!("{path}.{}", definition.name),
            );
            if known_declarations.insert((
                qualified_name.clone(),
                definition.span.definition_order,
                definition.span.start,
            )) {
                self.runtime_inner_constructor_keys.insert((
                    qualified_name,
                    definition.span.definition_order,
                    definition.span.start,
                ));
                all_structs.push((definition, module_path, false));
            }
        }

        // Register every declared type name (struct / abstract / primitive) in the
        // VM-local type-name registry BEFORE any method specificity / dispatch
        // scoring runs, so `is_type_variable_name` never misclassifies a declared
        // type whose spelling matches the type-variable shape (`S2`, `W1`, `Q7`)
        // as an unbound type variable (Issue #9464). This must precede method
        // compilation: the frozen candidate specificity for operators like
        // `convert(::Type{W1{T}}, w::W1)` is computed here, and a misclassified
        // `w::W1` param loses to the abstract `x::Real` fallback -> wrong dispatch
        // / StackOverflow. The registry is populated later too (on
        // `StructHierarchy::insert`), but that is after this scoring point.
        for (struct_def, module_path, _) in &all_structs {
            let declared_name = module_path.as_ref().map_or_else(
                || struct_def.name.clone(),
                |path| format!("{}.{}", path, struct_def.name),
            );
            crate::types::register_type_name(&declared_name);
        }
        for at in &program.abstract_types {
            crate::types::register_type_name(&at.name);
        }
        for pt in &program.primitive_types {
            crate::types::register_type_name(&pt.name);
        }

        let mut module_abstract_names: HashMap<String, HashSet<String>> = HashMap::new();
        for module in all_modules {
            collect_module_abstract_names(module, "", &mut module_abstract_names);
            super::collect::collect_module_runtime_nominal_names(
                module,
                "",
                &mut self.shared_ctx.runtime_nominal_callable_names,
            );
        }

        // Build a map of module_path -> set of struct names defined in that module.
        // This is used to qualify struct type names in function parameters for module functions.
        let mut module_struct_names: HashMap<String, HashSet<String>> = HashMap::new();
        for (struct_def, module_path, _) in &all_structs {
            if let Some(path) = module_path {
                module_struct_names
                    .entry(path.clone())
                    .or_default()
                    .insert(struct_def.name.clone());
            }
        }

        // Process all structs (top-level and module structs)
        let struct_tables_build_timer = profile::start("compile.struct_tables_build");
        for (struct_def, module_path, prefer_base_origin) in &all_structs {
            // Determine the struct name (qualified for module structs)
            let struct_name = match module_path {
                Some(path) => format!("{}.{}", path, struct_def.name),
                None => struct_def.name.clone(),
            };
            let parent_type = match module_path {
                Some(path) => qualify_module_local_parent_type(
                    struct_def.parent_type.clone(),
                    path,
                    &module_abstract_names,
                ),
                None => struct_def.parent_type.clone(),
            };

            // When using cache, skip Base structs that are already registered.
            // This prevents re-assigning type_ids and breaking cached bytecode.
            if precompiled_base.is_some() && struct_table.contains_key(&struct_name) {
                // For parametric structs, still register them in parametric_structs
                // but don't modify struct_table or struct_defs
                if struct_def.is_parametric() {
                    let mut stored_def = (*struct_def).clone();
                    stored_def.parent_type = parent_type;
                    parametric_structs.insert(
                        struct_name.clone(),
                        ParametricStructDef {
                            def: stored_def.clone(),
                        },
                    );
                    // Top-level bundled Base structs are represented by bare
                    // IR names. Preserve their explicit `Base.T` owner as a
                    // second registry binding before a user module can replace
                    // the source-visible bare alias (Issue #11369).
                    if module_path.is_none() && stored_def.is_base_origin {
                        base_parametric_structs.insert(
                            struct_name.clone(),
                            ParametricStructDef {
                                def: stored_def.clone(),
                            },
                        );
                    }
                    // Issue #10341: mirror the non-cached lane's short-name
                    // registration so bare `Point{T}` resolution does not
                    // depend on the cache mode.
                    if module_path.is_some() {
                        parametric_structs.insert(
                            struct_def.name.clone(),
                            ParametricStructDef { def: stored_def },
                        );
                    }
                } else if module_path.is_some() {
                    // Issue #11046: the owner index preserves both declarations;
                    // this insertion only changes the lexical bare alias.
                    let _ = struct_table.insert_alias(struct_def.name.clone(), &struct_name);
                }
                continue;
            }

            if struct_def.is_parametric() {
                let mut stored_def = (*struct_def).clone();
                stored_def.parent_type = parent_type;
                // Store parametric struct definition for later instantiation
                // All parametric structs (including Complex) are handled the same way
                parametric_structs.insert(
                    struct_name.clone(),
                    ParametricStructDef {
                        def: stored_def.clone(),
                    },
                );
                if module_path.is_none() && stored_def.is_base_origin {
                    base_parametric_structs.insert(
                        struct_name.clone(),
                        ParametricStructDef {
                            def: stored_def.clone(),
                        },
                    );
                }
                // Also register with short name for module structs
                // This allows `Point(...)` syntax after `using .MyGeometry`
                if module_path.is_some() {
                    parametric_structs.insert(
                        struct_def.name.clone(),
                        ParametricStructDef { def: stored_def },
                    );
                }
            } else {
                // Concrete struct - register immediately with sequential type_id
                let type_id = next_type_id;
                next_type_id += 1;

                let fields: Vec<(String, ValueType)> = struct_def
                    .fields
                    .iter()
                    .map(|f| {
                        // Issue #4856: `StructField::as_julia_type` only returns
                        // `Some` for `TypeExpr::Concrete`, so a user-struct-typed
                        // field like `inner::InnerProbe` (parsed as
                        // `TypeExpr::Named`) was falling through to
                        // `ValueType::Any`. As a result the inference engine saw
                        // `OuterProbe.inner` as `Any`, and `x.inner.value` widened
                        // to `Any` because the nested struct identity was lost on
                        // the way into the lattice struct table. Resolve any
                        // typed field through `TypeExpr::to_julia_type_lossy`
                        // so struct-typed fields land as
                        // `ValueType::Struct(type_id)` whenever the field's
                        // struct is already registered in `struct_table`.
                        let jt = f
                            .as_julia_type()
                            .or_else(|| f.type_expr.as_ref().map(TypeExpr::to_julia_type_lossy));
                        let vt = jt
                            .as_ref()
                            .map(|jt| {
                                // Abstract numeric fields keep an Any storage
                                // tag so the original runtime value survives
                                // (Issue #11407).
                                crate::compile::type_helpers::field_declared_value_type_scoped(
                                    jt,
                                    &struct_table,
                                    *prefer_base_origin,
                                )
                            })
                            .unwrap_or(ValueType::Any); // Untyped fields are Any (Julia semantics)
                                                        // Issue #5125: the reflection `Method` struct exposes a
                                                        // `.module::Module` field, but `module` is a reserved keyword
                                                        // the parser cannot accept as a field name, so the pure-Julia
                                                        // definition declares it as `mod`. Canonicalize it to
                                                        // `module` here (once, at the single field-table build site)
                                                        // so `m.module` field access resolves through `struct_table`
                                                        // and `struct_defs` consistently and `fieldnames(Method)`
                                                        // reports `:module`, matching upstream.
                        let field_name = if struct_name == "Method" && f.name == "mod" {
                            "module".to_string()
                        } else {
                            f.name.clone()
                        };
                        (field_name, vt)
                    })
                    .collect();
                let field_julia_types: Vec<JuliaType> = struct_def
                    .fields
                    .iter()
                    .map(|f| {
                        f.as_julia_type()
                            .or_else(|| f.type_expr.as_ref().map(TypeExpr::to_julia_type_lossy))
                            .unwrap_or(JuliaType::Any)
                    })
                    .collect();

                let has_inner_ctor = !struct_def.inner_constructors.is_empty();
                struct_table.insert(
                    struct_name.clone(),
                    StructInfo {
                        type_id,
                        is_mutable: struct_def.is_mutable,
                        fields: fields.clone(),
                        has_inner_constructor: has_inner_ctor,
                    },
                );
                // Also register with short name for module structs
                if module_path.is_some() {
                    let _ = struct_table.insert_alias(struct_def.name.clone(), &struct_name);
                }

                // Push to struct_defs for all structs
                // Complex is already at index 0, so update it; others get new indices
                if struct_def.name == "Complex" {
                    // Update the placeholder at index 0 with actual definition
                    // Use "Complex{Float64}" as the name for proper runtime dispatch matching
                    // Methods like +(::Real, ::Complex{Float64}) need to match correctly
                    struct_defs[0] = StructDefInfo {
                        name: "Complex{Float64}".to_string(),
                        is_mutable: struct_def.is_mutable,
                        fields,
                        field_julia_types,
                        parent_type,
                    };
                } else {
                    struct_defs.push(StructDefInfo {
                        name: struct_name,
                        is_mutable: struct_def.is_mutable,
                        fields,
                        field_julia_types,
                        parent_type,
                    });
                }
            }
        }
        profile::finish(struct_tables_build_timer);

        // Build abstract type definitions (Issue #2523: preserve type_params at runtime).
        // Abstract types declared inside modules / bundled packages (Issues #7263 /
        // #7265) live only on `Module.abstract_types`; collect them alongside the
        // top-level ones so a module-local abstract annotation (`f(d::Distribution)`)
        // resolves to the abstract type instead of a concrete `Struct("Distribution")`
        // that no value satisfies.
        let mut all_abstract_type_defs: Vec<crate::ir::core::AbstractTypeDef> =
            program.abstract_types.clone();
        collect_module_abstract_types(opt_modules, &mut all_abstract_type_defs);

        let current_abstract_types: Vec<AbstractTypeDefInfo> = all_abstract_type_defs
            .iter()
            .map(|at| AbstractTypeDefInfo {
                name: at.name.clone(),
                parent: at.parent.clone(),
                type_params: at.type_params.clone(),
            })
            .collect();

        let repl_relocatable_nominal_tail = self.repl_current_function_count.is_some();
        let mut abstract_types: Vec<AbstractTypeDefInfo> = if repl_relocatable_nominal_tail {
            precompiled_base
                .map(|base| base.abstract_types.clone())
                .unwrap_or_default()
        } else {
            current_abstract_types.clone()
        };
        if repl_relocatable_nominal_tail {
            for definition in &current_abstract_types {
                if abstract_types
                    .iter()
                    .all(|prior| prior.name != definition.name)
                {
                    abstract_types.push(definition.clone());
                }
            }
        }

        // Build set of abstract type names for compiler
        let mut abstract_type_names: HashSet<String> = all_abstract_type_defs
            .iter()
            .map(|at| at.name.clone())
            .collect();

        // Build user-declared primitive types (`primitive type Name Bits end`, Issue #5058).
        // These carry the declared bit width and optional abstract supertype so the
        // runtime type-reflection layer can answer isprimitivetype/isbitstype/sizeof/
        // supertype/<: for them. Modules can also declare primitive types, so collect
        // those too.
        let current_primitive_types: Vec<PrimitiveTypeDefInfo> = program
            .primitive_types
            .iter()
            .map(|pt| PrimitiveTypeDefInfo {
                name: pt.name.clone(),
                parent: pt.parent.clone(),
                bits: pt.bits,
            })
            .collect();
        let mut current_primitive_types = current_primitive_types;
        collect_module_primitive_types(opt_modules, &mut current_primitive_types);
        let mut primitive_types: Vec<PrimitiveTypeDefInfo> = if repl_relocatable_nominal_tail {
            precompiled_base
                .map(|base| base.primitive_types.clone())
                .unwrap_or_default()
        } else {
            current_primitive_types.clone()
        };
        if repl_relocatable_nominal_tail {
            for definition in &current_primitive_types {
                if primitive_types
                    .iter()
                    .all(|prior| prior.name != definition.name)
                {
                    primitive_types.push(definition.clone());
                }
            }
        }

        // Fold in the abstract / primitive type defs carried by the precompiled
        // base/prefix, dedup'd by name with this program's own defs winning
        // (Issue #9701). The struct path already seeds `struct_defs` from the
        // cache above; without the symmetric fold, a compile that reuses a
        // prefix whose IR is NOT re-merged into `program` — the REPL
        // input-delta compile, whose `precompiled_base` is the session's
        // accumulated prefix (Issue #9199 S5/LV2) — cannot resolve a prior-eval
        // `abstract type` / `primitive type` name: `d isa Animal` compiled to a
        // dynamic global load and raised UndefVarError at runtime, and an
        // `f(a::Animal)` delta method lost its abstract annotation. For the
        // plain Base-cache path this is a no-op: `merge_with_precompiled_base`
        // already merged the same defs into `program`, so every name is
        // already present.
        if let Some(base_cache) = precompiled_base {
            for at in &base_cache.abstract_types {
                if !abstract_type_names.contains(&at.name) {
                    abstract_type_names.insert(at.name.clone());
                    if abstract_types.iter().all(|prior| prior.name != at.name) {
                        abstract_types.push(at.clone());
                    }
                    crate::types::register_type_name(&at.name);
                }
            }
            for pt in &base_cache.primitive_types {
                if primitive_types.iter().all(|p| p.name != pt.name) {
                    crate::types::register_type_name(&pt.name);
                    primitive_types.push(pt.clone());
                }
            }
        }

        self.all_structs = all_structs;
        self.module_struct_names = module_struct_names;
        self.abstract_types = abstract_types;
        self.abstract_type_names = abstract_type_names;
        self.primitive_types = primitive_types;
        StructTables {
            struct_table,
            parametric_structs,
            base_parametric_structs,
            struct_defs,
            next_type_id,
            cached_instantiation_table,
        }
    }

    fn init_shared_context(&mut self, tables: StructTables) {
        let StructTables {
            struct_table,
            parametric_structs,
            base_parametric_structs,
            struct_defs,
            next_type_id,
            cached_instantiation_table,
        } = tables;
        let program = self.program;
        let opt_modules = self.opt_modules;
        let opt_main = self.opt_main;
        let all_modules = &self.all_modules;
        let global_types = self.global_types;
        let global_struct_names = self.global_struct_names;
        let cached_closure_captures = self.cached_closure_captures;
        let abstract_types = &self.abstract_types;
        let primitive_types = &self.primitive_types;

        // Create shared compilation context
        // When using cache, pass the cached instantiation table to prevent re-instantiation
        let shared_ctx_init_timer = profile::start("compile.shared_ctx_init");
        self.shared_ctx = if !cached_instantiation_table.is_empty() {
            SharedCompileContext::with_instantiation_table(
                struct_table,
                struct_defs,
                parametric_structs,
                base_parametric_structs,
                abstract_types.clone(),
                next_type_id,
                cached_instantiation_table,
            )
        } else {
            SharedCompileContext::new(
                struct_table,
                struct_defs,
                parametric_structs,
                base_parametric_structs,
                abstract_types.clone(),
                next_type_id,
            )
        };
        self.shared_ctx.repl_source_ordered_dispatch = self.repl_current_function_count.is_some();
        let shared_ctx = &mut self.shared_ctx;
        // Issues #11025/#11117: record only types declared by the CURRENT source.
        // Base/precompiled definitions were lowered in a different source-order
        // coordinate space; comparing their ordinals with this input can turn an
        // already-visible type such as Rational into a false forward reference.
        collect_current_type_definition_positions(
            &self.all_structs,
            self.opt_modules,
            self.program,
            self.precompiled_base,
            self.repl_current_struct_count,
            self.current_input_type_names.as_ref(),
            &mut shared_ctx.type_definition_positions,
        );
        shared_ctx.has_opaque_runtime_eval = self.has_opaque_runtime_eval;
        shared_ctx.opaque_runtime_eval_function_names =
            self.opaque_runtime_eval_function_names.clone();
        shared_ctx
            .runtime_nominal_callable_names
            .extend(self.pre_optimization_runtime_nominal_names.iter().cloned());
        shared_ctx.current_input_runtime_nominal_names =
            self.pre_optimization_runtime_nominal_names.clone();

        // `@enum` pre-pass (Issue #5139): collect every enum definition up front so
        // that bare references to an enum type name or its members resolve no matter
        // where they appear relative to the `@enum`, and so call sites can recognize
        // `Color(value)` construction and `instances(Color)`. Enum defs lower to
        // `Stmt::EnumDef` inside `main` (or a module body), so scan blocks directly.
        if let Some(prefix) = self.precompiled_base {
            for definition in &prefix.enum_defs {
                shared_ctx.enum_types.insert(
                    definition.name.clone(),
                    EnumInfo {
                        base_type: definition.base_type.clone(),
                        members: definition.members.clone(),
                    },
                );
            }
        }
        collect_enum_types(opt_main, &mut shared_ctx.enum_types);
        for module in opt_modules {
            collect_enum_types_in_module(module, &mut shared_ctx.enum_types);
        }

        // Register user-declared primitive types so bare references resolve to a
        // DataType value and type reflection can answer isprimitivetype/sizeof/
        // supertype (Issue #5058).
        shared_ctx.set_primitive_types(primitive_types.clone());

        // Populate type aliases from program. A *bare* reference to a parametric
        // alias (`MyVec` for `MyVec{T} = Vector{T}`) resolves to the target's bare
        // base type (`Vector`), matching upstream which prints/compares the alias as
        // the underlying `UnionAll`. Parametric *uses* (`MyVec{Int}`) are expanded
        // during lowering instead (Issue #5055).
        //
        // Base-level const type aliases (e.g. `const ComplexF64 =
        // Complex{Float64}` from complex.jl) live in the prelude program, which
        // is independent of the base bytecode cache. Register them FIRST so
        // bare references like `ComplexF64` resolve as DataType values; user
        // aliases of the same name register afterwards and override (later
        // definition wins, matching upstream). Without this, base const aliases
        // were dropped on the cached-base path (Issue #5065). Note this table
        // is flat and unqualified — it does not model upstream's export
        // filtering, so non-exported Base aliases leak into Main (`Bottom` was
        // removed from the prelude for exactly that reason, Issue #10304; the
        // general leak is tracked by Issue #10578).
        if let Some(prelude) = crate::get_prelude_program() {
            for alias in &prelude.type_aliases {
                register_type_alias(shared_ctx, alias);
            }
        }
        for alias in &program.type_aliases {
            register_type_alias(shared_ctx, alias);
        }
        for module in all_modules {
            register_module_type_aliases(shared_ctx, module, "");
        }

        // Issue #11113: a const alias whose target is a Base/stdlib-declared
        // struct (`const MyPair = Pair`) never produces a `TypeAliasDef`
        // above. Lowering's alias gate (`is_likely_type_name`) only
        // recognizes types the CURRENT program declares (Issue #11104) plus a
        // fixed builtin-name list; Base is lowered in an isolated pass (or,
        // under the Base cache, never lowered from source at all), so a Base
        // struct's name never reaches that gate and `MyPair` registers no
        // alias. `struct_table` (just built above, in BOTH cache modes) DOES
        // know every registered struct, so resolve any still-unregistered
        // bare-identifier const/global binding through it here instead of
        // maintaining a name list that would need a new entry for every Base
        // struct (SubString, Regex, RegexMatch, ...) and would still miss
        // third-party/package structs entirely.
        register_struct_table_backed_aliases(
            &mut shared_ctx.type_aliases,
            &shared_ctx.struct_table,
            &opt_main.stmts,
        );
        for module in all_modules {
            register_struct_table_backed_module_aliases(
                &mut shared_ctx.type_aliases,
                &shared_ctx.struct_table,
                module,
                "",
            );
        }

        // Pre-populate closure captures from cache (Issue #2100)
        // When using the compilation cache, outer Base functions are skipped (cached bytecode).
        // But their inner/nested functions still need to be compiled, and they reference
        // captured variables from the outer scope. Without this, those inner functions
        // would get empty closure_captures and fail with "Undefined variable" errors.
        if let Some(cached_captures) = cached_closure_captures {
            shared_ctx.closure_captures = cached_captures.clone();
        }

        profile::finish(shared_ctx_init_timer);

        // Store global_types temporarily - will resolve after struct_table is built
        self.pending_global_types = global_types.clone();
        self.pending_global_struct_names = global_struct_names.clone();
    }

    fn seed_outputs_from_cache(&mut self) {
        let precompiled_base = self.precompiled_base;
        let cached_method_tables = self.cached_method_tables;

        // Build method tables from functions (including module functions)
        // Start with cached Base method tables if available (Option A optimization)
        //
        // Issue #10113: this already restores each cached Base function's full
        // `MethodTable` (methods, dedup state, everything but the per-compile
        // struct-hierarchy projection) via `clone_for_reprojection`, which
        // `Arc::clone`s the shared `methods` vector instead of rebuilding it
        // through `add_method` — see `cached_base_method_table_reuses_shared_arc_10113`
        // in `cache.rs` for the regression pin. `build_method_tables`'s
        // per-function loop below never calls `add_method` for a genuinely
        // cached Base function either (it `continue`s before reaching that
        // code), so no Base method table is rebuilt from scratch on a
        // cache-hit compile. The residual per-compile cost here is the
        // unavoidable `HashMap<String, MethodTable>` re-materialization (owned
        // `String` keys cannot be shared without a broader `Rc<str>` key
        // change) — real but small (~0.1-0.2ms for ~1450 entries).
        self.method_tables = profile::time("compile.cached_method_tables_clone", || {
            if let Some(cached) = cached_method_tables {
                cached
                    .iter()
                    .map(|(name, table)| (name.clone(), table.clone_for_reprojection()))
                    .collect()
            } else {
                HashMap::new()
            }
        });

        // When using cache, initialize function_infos from cache to maintain consistent indices.
        // This is critical because cached bytecode contains Call instructions with indices that
        // must match function_infos. User functions are appended at the end.
        //
        // func_index_map: maps all_functions index -> function_infos index
        // - For Base functions (when using cache): identity mapping (0->0, 1->1, etc.)
        // - For user functions: maps to end of cache (e.g., 678->682 if cache has 682 entries)
        // Issue #9140: `functions` holds `Rc<FunctionInfo>`, so this clone is a
        // pointer-copy of ~4969 Rcs (microseconds), not a deep clone (~12 ms).
        let (function_infos, global_index, cached_base_len): (
            Vec<std::rc::Rc<FunctionInfo>>,
            usize,
            usize,
        ) = profile::time("compile.cached_function_infos_clone", || {
            if let Some(base_cache) = precompiled_base {
                let len = base_cache.functions.len();
                (base_cache.functions.clone(), len, len)
            } else {
                (Vec::new(), 0, 0)
            }
        });
        self.function_infos = function_infos;
        self.global_index = global_index;
        self.cached_base_len = cached_base_len;
        // When using cache, initialize show_methods from cached Base (Issue #2489).
        // Base show methods (e.g., show(io, Complex)) are skipped during the function loop
        // when using cache, so they must be pre-populated from the cached compilation.
        self.show_methods = profile::time("compile.cached_show_methods_clone", || {
            if let Some(base_cache) = precompiled_base {
                base_cache.show_methods.clone()
            } else {
                Vec::new()
            }
        });
        self.print_methods = profile::time("compile.cached_print_methods_clone", || {
            if let Some(base_cache) = precompiled_base {
                base_cache.print_methods.clone()
            } else {
                Vec::new()
            }
        });
    }

    fn collect_module_metadata(&mut self) {
        let program = self.program;
        let base_function_count = self.base_function_count;
        let opt_user_functions = self.opt_user_functions;
        let all_modules = &self.all_modules;
        let module_registry = &mut self.module_registry;
        let module_functions = &mut self.module_functions;
        let module_exports = &mut self.module_exports;
        let module_constants = &mut self.module_constants;
        let module_publics = &mut self.shared_ctx.module_publics;
        let imported_functions = &mut self.imported_functions;
        let toplevel_import_sources = &mut self.toplevel_import_sources;
        let extra_module_metadata = self.extra_module_metadata;

        // Build module function mapping: module_path -> set of function names
        // For nested modules, path is "A.B.C"

        // Collect info from all top-level modules (including precompiled stdlib).
        // `register_module_ids` walks the identical recursion as
        // `collect_module_info` (Issue #10988 Phase 2a), so every module path
        // registered here is also a `module_functions` key, in deterministic
        // source-declared order (never HashMap iteration order).
        for module in all_modules {
            collect_module_publics(module, "", module_publics);
            collect_module_info(
                module,
                "",
                module_functions,
                module_exports,
                module_constants,
            );
            register_module_ids(module, "", module_registry);
        }

        // REPL relocatable-delta (Issue #9199 LV5): fold in the surface of prior
        // modules realized on the reused prefix so `M.f()` / `M.const` in this
        // delta resolve (against the live VM's already-installed functions and
        // its module-constant globals) instead of erroring "Unknown module". This
        // is metadata ONLY — the module bodies are never re-emitted (they already
        // live in `precompiled_base`). `all_modules` is empty on a delta compile,
        // so a qualified name present here comes solely from the carried surface.
        // A missing/incomplete entry only downgrades to the full-recompile
        // fallback (the delta compile then errors ⇒ `Ok(None)`), never a
        // miscompile — the function index is resolved from the authoritative
        // prefix method tables, not from these name sets.
        if let Some(meta) = extra_module_metadata {
            for (path, names) in &meta.module_publics {
                module_publics.insert(path.clone(), names.clone());
            }
            for (path, funcs) in &meta.module_functions {
                module_functions
                    .entry(path.clone())
                    .or_default()
                    .extend(funcs.iter().cloned());
            }
            for (path, exports) in &meta.module_exports {
                module_exports
                    .entry(path.clone())
                    .or_default()
                    .extend(exports.iter().cloned());
            }
            for (path, consts) in &meta.module_constants {
                module_constants
                    .entry(path.clone())
                    .or_default()
                    .extend(consts.iter().cloned());
            }
        }

        // Build set of function names that are imported via `using`
        // This respects both export restrictions and selective imports
        //
        // Issue #10130: a non-selective `using Module` (the `None` arm below)
        // merges the module's ENTIRE exported/full function-name set into
        // `imported_functions`. Re-merging the same module's set again is a
        // no-op (the destination is a `HashSet`), so when the same module is
        // imported non-selectively more than once — e.g. several `using Base`
        // occurrences, or multiple modules each `using Base` — this memoizes
        // the merge per resolved module name instead of re-walking the whole
        // set (up to ~5000 Base function names) on every repeat.
        let mut import_all_merged: HashSet<String> = HashSet::new();
        for using_import in &program.usings {
            let module_name = resolve_using_module_name(using_import, "", module_functions);

            // Get the functions available in this module
            if let Some(module_funcs) = module_name
                .as_deref()
                .and_then(|name| module_functions.get(name))
            {
                // Get the exported functions (empty = all exported)
                let exports = module_name
                    .as_deref()
                    .and_then(|name| module_exports.get(name));
                match &using_import.symbols {
                    // Selective import: `using Module: func1, func2`
                    Some(symbols) => {
                        for sym in symbols {
                            imported_functions.insert(sym.clone());
                            // Record the source module so a later top-level
                            // `function sym(...)` extends `Module.sym` (joins the
                            // qualified table too), not just shadows the bare `sym`
                            // (Issue #8052).
                            if let Some(src) = module_name.as_deref() {
                                toplevel_import_sources
                                    .entry(sym.clone())
                                    .or_insert_with(|| vec![src.to_string()]);
                            }
                        }
                    }
                    // Import all exported: `using Module`
                    None if !using_import.is_import => {
                        let already_merged = module_name
                            .as_deref()
                            .is_some_and(|name| !import_all_merged.insert(name.to_string()));
                        if !already_merged {
                            if let Some(exports) = exports.filter(|exports| !exports.is_empty()) {
                                imported_functions.extend(exports.iter().cloned());
                            } else {
                                for func_name in module_funcs {
                                    imported_functions.insert(func_name.clone());
                                }
                            }
                        }
                    }
                    None => {}
                }
            }
        }

        // Add top-level functions to imported_functions (they're always available)
        for func in program
            .functions
            .iter()
            .take(base_function_count)
            .map(|f| f.as_ref())
            .chain(opt_user_functions.iter())
        {
            imported_functions.insert(func.name.clone());
        }

        // REPL input-delta (Issue #9199 S5): prior user functions live in the
        // reused precompiled prefix, not this compile's IR, so add their names
        // here or a call to a prior-defined function is rejected as "not
        // imported". `None` for every non-delta compile.
        if let Some(extra) = self.extra_imported_functions {
            imported_functions.extend(extra.iter().cloned());
        }

        // For backward compatibility, also keep track of used module names.
        self.usings_set = program.usings.iter().map(|u| u.module.clone()).collect();
    }

    fn validate_using_imports(&self) -> CResult<()> {
        validate_scope_using_imports(&self.program.usings, &self.module_functions)?;
        for module in &self.all_modules {
            validate_module_using_imports(module, &self.module_functions)?;
        }
        Ok(())
    }

    fn register_imported_type_aliases(&mut self) {
        record_scope_value_binding_positions(&mut self.shared_ctx, &self.program.main, "");
        register_scope_imported_type_aliases(
            &mut self.shared_ctx,
            &self.program.usings,
            "",
            &self.module_functions,
        );
        for module in &self.all_modules {
            register_module_imported_type_aliases(
                &mut self.shared_ctx,
                module,
                "",
                &self.module_functions,
            );
        }
    }

    fn build_function_universe(&mut self) {
        let program = self.program;
        let base_function_count = self.base_function_count;
        let opt_user_functions = self.opt_user_functions;
        let opt_main = self.opt_main;
        let inline_functions = self.inline_functions;
        let base_lifted_inline_indices = &self.base_lifted_inline_indices;
        let module_scope_overrides = self.module_scope_overrides;
        let all_modules = &self.all_modules;
        let imported_functions = &mut self.imported_functions;

        // Collect all functions in Julia evaluation order for `using`-loaded stdlib:
        // prelude/Base first, loaded module methods next, then user top-level
        // methods. This lets user methods written after `using LinearAlgebra`
        // replace same-signature stdlib methods in the method table.
        let base_function_names: HashSet<String> = program
            .functions
            .iter()
            .take(base_function_count)
            .map(|func| func.name.clone())
            .collect();
        let mut user_function_names: HashSet<String> = opt_user_functions
            .iter()
            .map(|func| func.name.clone())
            .collect();
        let mut user_module_functions = Vec::new();
        for module in self.opt_modules {
            collect_module_functions(module, "", &mut user_module_functions);
        }
        user_function_names.extend(
            user_module_functions
                .iter()
                .map(|(func, _)| func.name.clone()),
        );
        let base_functions = program.functions.iter().take(base_function_count);
        let user_functions = opt_user_functions.iter();
        let mut all_functions: Vec<(&Function, Option<String>)> =
            base_functions.map(|f| (f.as_ref(), None)).collect();
        let mut base_cached_extra_function_indices: HashSet<usize> = HashSet::new();

        let user_module_count = self.opt_modules.len();
        for (module_index, module) in all_modules.iter().enumerate() {
            let before = all_functions.len();
            collect_module_functions(module, "", &mut all_functions);
            if module_index >= user_module_count {
                base_cached_extra_function_indices.extend(before..all_functions.len());
            }
        }

        // Map each module-level function's QUALIFIED name ("Module.path.func",
        // matching the parent identity `collect_from_module` now assigns to its
        // nested functions, Issue #10214) to its owning module path, so that
        // nested/closure functions lifted from a module function body can inherit
        // the same module scope for name resolution (Issue #7180). Without this, a
        // closure passed to a Base HOF inside a module (e.g.
        // `findfirst(x -> help(x, 2), v)`) is registered with `module_path = None`
        // and fails to resolve the module-private helper `help`.
        //
        // Keying by the QUALIFIED name (not the bare `func.name`) is required so
        // two different modules defining a same-named top-level function (e.g.
        // both declaring `function outer() ... end`) get distinct entries here —
        // a bare-name key would let whichever module's `outer` was collected last
        // silently overwrite the other's, misattributing ITS nested helpers'
        // module scope to the wrong module (Issue #10214).
        let mut function_module_paths: HashMap<String, String> = all_functions
            .iter()
            .filter_map(|(func, module_path)| {
                module_path
                    .as_ref()
                    .map(|path| (format!("{}.{}", path, func.name), path.clone()))
            })
            .collect();
        // Nested-function tracking (Issue #1743) + the Issue #9245 two-region
        // classification. First pass, name-based bookkeeping only (order matters
        // for module-scope propagation through nested parents, e.g.
        // `f#__do_block_0 -> f#__do_block_0#__lambda_0`, Issue #7591/#7180):
        // resolve each lifted inline function's module scope and record it, and
        // mark it imported so it can be called. No `all_functions` push yet —
        // the placement below splits by scope.
        let mut inline_scopes: Vec<Option<String>> = Vec::with_capacity(inline_functions.len());
        // `inline_functions` index -> "this occurrence's scope came from
        // `module_scope_overrides`", i.e. it is a lexically-scoped
        // `let`/`@testset` ROOT helper, not a genuine top-level
        // `module.functions` declaration (Issue #10236). Threaded through to
        // `module_body_scoped_root_indices` (the `all_functions`-index form)
        // below once each entry's final placement is known.
        let mut module_body_scoped_root_inline_indices: HashSet<usize> = HashSet::new();
        for (inline_idx, (func, parent_name)) in inline_functions.iter().enumerate() {
            // Issue #10073: a function collected directly from a module-body
            // `let`/`@testset` (no enclosing named-function parent) has no
            // `function_module_paths` entry to inherit from — it IS the root
            // of its own module-scope chain. `module_scope_overrides` (built
            // alongside `inline_functions` by `collect_from_module`, keyed by
            // this function's OWN index in `inline_functions` — Issue
            // #10214/#10236, not by bare name, so a module-body root and an
            // unrelated same-named Main-level `let` root cannot collide)
            // supplies that root scope directly; nested descendants still
            // resolve through the ordinary `function_module_paths` parent-name
            // lookup below once this root's scope is recorded.
            let inline_module_path = parent_name
                .as_ref()
                .and_then(|parent| function_module_paths.get(parent).cloned())
                .or_else(|| module_scope_overrides.get(&inline_idx).cloned());
            if parent_name.is_none() && module_scope_overrides.contains_key(&inline_idx) {
                module_body_scoped_root_inline_indices.insert(inline_idx);
            }
            let inline_name = if let Some(parent) = parent_name {
                let qualified_name = format!("{}#{}", parent, func.name);
                imported_functions.insert(qualified_name.clone());
                qualified_name
            } else {
                imported_functions.insert(func.name.clone());
                func.name.clone()
            };
            if let Some(ref module_path) = inline_module_path {
                function_module_paths.insert(inline_name, module_path.clone());
            }
            inline_scopes.push(inline_module_path);
        }

        // Issue #9245: place MODULE-scoped inline closures right after the module
        // functions, BEFORE user functions, so the package region (module
        // functions + module closures) stays contiguous right after Base. A
        // lifted anonymous lambda / top-level user function (the #9158 iOS
        // Surface sample's `(x, y) -> …` argument lowers to one) then cannot
        // interpose into the package region and shift the package closures —
        // which would deactivate the preload cache's `closure_layout` gate
        // (#9230). Base/user/main closures (module scope `None`) stay in the
        // trailing block below: closures inside user functions need
        // parent-before-child compile order, so moving them ahead of their
        // parent user function breaks capture resolution (moving the WHOLE
        // inline block regressed 10 closure-capture tests; module closures'
        // parents are module functions, compiled early, so moving only those is
        // safe). `func_idx_to_parent` is built with the real all_functions index
        // here rather than `inline_start_idx + inline_idx` (the inline block is
        // no longer a single contiguous run).
        let mut func_idx_to_parent: HashMap<usize, String> = HashMap::new();
        let mut module_body_scoped_root_indices: HashSet<usize> = HashSet::new();
        for (inline_idx, ((func, parent_name), scope)) in inline_functions
            .iter()
            .zip(inline_scopes.iter())
            .enumerate()
        {
            if scope.is_some() {
                let idx = all_functions.len();
                all_functions.push((func, scope.clone()));
                if base_lifted_inline_indices.contains(&inline_idx) {
                    base_cached_extra_function_indices.insert(idx);
                }
                if let Some(parent) = parent_name {
                    func_idx_to_parent.insert(idx, parent.clone());
                }
                if module_body_scoped_root_inline_indices.contains(&inline_idx) {
                    module_body_scoped_root_indices.insert(idx);
                }
            }
        }

        let first_user_function_idx = all_functions.len();
        all_functions.extend(user_functions.map(|f| (f, None)));

        // The trailing inline block: base/user/main closures (module scope
        // `None`), kept in their original relative position (after user
        // functions) so their parent-before-child compile order is preserved.
        // `inline_start_idx` names THIS block, so `is_user_function_scope`
        // classifies user functions (idx < here, >= first_user) via its
        // `first_user_function_idx` arm and the early module closures (idx <
        // first_user) as non-user-scope — both correct.
        let inline_start_idx = all_functions.len();
        for (inline_idx, ((func, parent_name), scope)) in inline_functions
            .iter()
            .zip(inline_scopes.iter())
            .enumerate()
        {
            if scope.is_none() {
                let idx = all_functions.len();
                all_functions.push((func, None));
                if base_lifted_inline_indices.contains(&inline_idx) {
                    base_cached_extra_function_indices.insert(idx);
                }
                if let Some(parent) = parent_name {
                    func_idx_to_parent.insert(idx, parent.clone());
                }
            }
        }

        let callable_typeof_aliases =
            collect_callable_typeof_aliases(&opt_main.stmts, &all_functions);

        self.base_function_names = base_function_names;
        self.user_function_names = user_function_names;
        self.first_user_function_idx = first_user_function_idx;
        self.inline_start_idx = inline_start_idx;
        self.func_idx_to_parent = func_idx_to_parent;
        self.module_body_scoped_root_indices = module_body_scoped_root_indices;
        self.base_cached_extra_function_indices = base_cached_extra_function_indices;
        self.callable_typeof_aliases = callable_typeof_aliases;
        self.all_functions = all_functions;
    }

    fn prepopulate_closure_captures(&mut self) {
        let program = self.program;
        let base_function_count = self.base_function_count;
        let opt_user_functions = self.opt_user_functions;
        let inline_functions = self.inline_functions;
        let shared_ctx = &mut self.shared_ctx;

        // Pre-populate closure captures for nested functions (Issue #2100)
        //
        // When using prelude cache, parent functions are skipped during compilation,
        // so Stmt::FunctionDef in parent bodies never runs and closure captures are
        // never analyzed. This causes "Undefined variable" errors for captured variables
        // in nested functions that act as closures (e.g., curried string search functions).
        //
        // Fix: analyze free variables for all nested functions upfront by examining
        // each parent function's parameters as the outer scope.
        profile::time("compile.prepopulate_closure_captures", || {
            let parent_params_by_name: HashMap<String, HashSet<String>> =
                if inline_functions.iter().any(|(_, parent)| parent.is_some()) {
                    let mut parent_params_by_name = HashMap::new();
                    for parent_func in program
                        .functions
                        .iter()
                        .take(base_function_count)
                        .map(|f| f.as_ref())
                        .chain(opt_user_functions.iter())
                    {
                        parent_params_by_name
                            .entry(parent_func.name.clone())
                            .or_insert_with(|| {
                                parent_func.params.iter().map(|p| p.name.clone()).collect()
                            });
                    }
                    parent_params_by_name
                } else {
                    HashMap::new()
                };

            for (nested_func, parent_name) in inline_functions {
                if let Some(parent) = parent_name {
                    if let Some(outer_vars) = parent_params_by_name.get(parent) {
                        let free_vars = analyze_free_variables(nested_func, outer_vars);
                        if !free_vars.is_empty() {
                            let qname = format!("{}#{}", parent, nested_func.name);
                            shared_ctx.closure_captures.insert(qname, free_vars);
                        }
                    }
                }
            }
        });
    }

    fn preinstantiate_parametric_types(&mut self) {
        let base_function_count = self.base_function_count;
        let opt_main = self.opt_main;
        let precompiled_base = self.precompiled_base;
        let all_functions = &self.all_functions;
        let shared_ctx = &mut self.shared_ctx;

        // Pre-instantiate parametric struct types used in function parameters
        // This ensures that types like Complex{Float64} are in struct_table
        // BEFORE we infer function return types for method tables
        profile::time("compile.preinstantiate_parametric_params", || {
            for (idx, (func, _)) in all_functions.iter().enumerate() {
                if precompiled_base.is_some() && idx < base_function_count {
                    continue;
                }

                // Collect type parameter names from the function's where clause
                let type_param_names: HashSet<&str> =
                    func.type_params.iter().map(|tp| tp.name.as_str()).collect();

                for param in &func.params {
                    let param_ty = param.effective_type();
                    if let JuliaType::Struct(name) = &param_ty {
                        if let Some(brace_idx) = name.find('{') {
                            let base_name = &name[..brace_idx];
                            let type_args_str = &name[brace_idx + 1..name.len() - 1];

                            // Check if any type argument is a type parameter from where clause
                            // e.g., Rational{T} where T - T is a type parameter, not a concrete type
                            let Some(type_args) = parse_type_args_recursive(type_args_str) else {
                                continue;
                            };
                            let has_type_param = type_args
                                .iter()
                                .any(|arg| type_expr_contains_type_param(arg, &type_param_names));

                            // Skip instantiation if any type arg is a where clause type parameter
                            // These will be instantiated at call sites with concrete types
                            if has_type_param {
                                continue;
                            }

                            // Instantiate the parametric struct type
                            let _ = shared_ctx
                                .resolve_instantiation_with_type_expr(base_name, &type_args);
                        }
                    }
                }
            }
        });

        // Collect struct literal types from main block and function bodies
        let struct_literal_names: HashSet<String> =
            profile::time("compile.collect_struct_literals", || {
                let mut struct_literal_names = HashSet::new();
                collect_struct_literal_types(&opt_main.stmts, &mut struct_literal_names);
                for (idx, (func, _)) in all_functions.iter().enumerate() {
                    if precompiled_base.is_some() && idx < base_function_count {
                        continue;
                    }
                    collect_struct_literal_types(&func.body.stmts, &mut struct_literal_names);
                }
                struct_literal_names
            });

        // Instantiate parametric struct types from literals
        profile::time("compile.instantiate_struct_literals", || {
            for struct_name in &struct_literal_names {
                if let Some(brace_idx) = struct_name.find('{') {
                    let base_name = &struct_name[..brace_idx];
                    let type_args_str = &struct_name[brace_idx + 1..struct_name.len() - 1];
                    let Some(type_args) = parse_type_args_recursive(type_args_str) else {
                        continue;
                    };
                    // Instantiate the type (ignore errors - may already exist)
                    let _ = shared_ctx.resolve_instantiation_with_type_expr(base_name, &type_args);
                }
            }
        });
    }

    fn resolve_global_types(&mut self) {
        let opt_main = self.opt_main;
        let all_modules = &self.all_modules;
        let pending_global_types = &self.pending_global_types;
        let pending_global_struct_names = &self.pending_global_struct_names;
        let shared_ctx = &mut self.shared_ctx;

        // Now that struct_table is fully built, resolve global_types from REPL session
        // Pre-collect global variable types from main block before function compilation.
        // This allows functions to reference top-level const/global variables with proper types.
        // Also collects const struct constructors for inlining in functions.
        {
            let mut global_types_map = std::mem::take(&mut shared_ctx.global_types);
            // Merge with provided global_types (from REPL session)
            // Resolve struct type_ids from struct_names using struct_table (now fully built)
            for (name, ty) in pending_global_types {
                if let ValueType::Struct(_) = ty {
                    // Resolve struct type_id from struct_name
                    if let Some(struct_name) = pending_global_struct_names.get(name) {
                        if let Some(struct_info) = shared_ctx.struct_table.get(struct_name) {
                            global_types_map
                                .insert(name.clone(), ValueType::Struct(struct_info.type_id));
                            continue;
                        }
                        // Recreate the exact parametric instance (e.g.
                        // `Rational{Int64}`) when this delta has only instantiated
                        // another member of the same family. The O(1) prefix index
                        // remains the cheap family-presence gate from Issue #10129,
                        // but its type id is not reusable: ids are program-local and
                        // a different instantiation may have different field facts.
                        if let Some(brace_idx) = struct_name.find('{') {
                            let prefix = &struct_name[..=brace_idx];
                            if struct_name.ends_with('}')
                                && shared_ctx
                                    .parametric_struct_prefix_index
                                    .contains_key(prefix)
                            {
                                let base_name = &struct_name[..brace_idx];
                                let args = &struct_name[brace_idx + 1..struct_name.len() - 1];
                                if let Some(type_args) = parse_type_args_recursive(args) {
                                    if let Ok(type_id) = shared_ctx
                                        .resolve_instantiation_with_type_expr(base_name, &type_args)
                                    {
                                        global_types_map
                                            .insert(name.clone(), ValueType::Struct(type_id));
                                        continue;
                                    }
                                }
                            }
                        }
                    }
                    // `ValueType::Struct` ids are local to one compiled
                    // program. A REPL-carried type that cannot be re-resolved
                    // in this delta must not retain the old id: that slot may
                    // denote an unrelated type here. Keep the runtime value
                    // generic so ordinary dispatch can use its actual StructRef
                    // identity (Issue #11147).
                    global_types_map.insert(name.clone(), ValueType::Any);
                    continue;
                }
                // Non-struct representations are stable across compiled programs.
                global_types_map.insert(name.clone(), ty.clone());
            }
            let mut global_const_structs = std::mem::take(&mut shared_ctx.global_const_structs);
            collect_global_types_for_inference(
                &opt_main.stmts,
                &mut global_types_map,
                &shared_ctx.struct_table,
                &mut global_const_structs,
            );
            shared_ctx.global_types = global_types_map;
            shared_ctx.global_const_structs = global_const_structs;
        }

        // Also collect global types from module bodies (for module-level constants like SHIFTEDMONTHDAYS).
        // This ensures module-level constants are registered before function compilation so they're
        // not flagged as "undefined variable" when referenced from module functions.
        {
            let mut global_types_map = std::mem::take(&mut shared_ctx.global_types);
            let mut global_const_structs = std::mem::take(&mut shared_ctx.global_const_structs);
            for module in all_modules {
                collect_global_types_for_inference(
                    &module.body.stmts,
                    &mut global_types_map,
                    &shared_ctx.struct_table,
                    &mut global_const_structs,
                );
            }
            shared_ctx.global_types = global_types_map;
            shared_ctx.global_const_structs = global_const_structs;
        }
    }

    fn resolve_module_imports(&mut self) {
        let all_modules = &self.all_modules;
        let module_functions = &self.module_functions;
        let module_exports = &self.module_exports;
        let module_imports_map = &mut self.module_imports_map;
        let module_usings_map = &mut self.module_usings_map;
        let module_imported_bindings = &mut self.shared_ctx.module_imported_bindings;

        // Collect module-level using statements to support module-local imports.
        let mut module_usings: HashMap<String, Vec<UsingImport>> = HashMap::new();

        for module in all_modules {
            collect_module_usings(module, "", &mut module_usings);
        }

        let top_level_resolved =
            resolve_scope_using_imports(&self.program.usings, "", module_functions);

        let mut live_binding_scopes = Vec::with_capacity(module_usings.len() + 1);
        live_binding_scopes.push(("Main".to_string(), top_level_resolved));
        let mut module_paths: Vec<_> = module_usings.keys().cloned().collect();
        module_paths.sort();
        for module_path in &module_paths {
            let resolved_usings = resolve_scope_using_imports(
                &module_usings[module_path],
                module_path,
                module_functions,
            );
            live_binding_scopes.push((module_path.clone(), resolved_usings));
        }
        register_all_live_import_bindings(
            module_imported_bindings,
            &live_binding_scopes,
            module_functions,
            module_exports,
        );

        // Resolve module-local imports based on their using statements.
        for (module_path, usings) in &module_usings {
            let mut imported = HashSet::new();
            for using_import in usings {
                if let Some(using_module) =
                    resolve_using_module_name(using_import, module_path, module_functions)
                {
                    if let Some(module_funcs) = module_functions.get(using_module.as_str()) {
                        let exports = module_exports.get(using_module.as_str());
                        let all_exported = exports.is_none_or(|e| e.is_empty());

                        match &using_import.symbols {
                            // Selective import: `using Module: func1, func2`
                            Some(symbols) => {
                                for sym in symbols {
                                    imported.insert(sym.clone());
                                    // Record the qualified re-export so a later
                                    // `ImportingModule.sym` resolves to its source
                                    // `using_module.sym` (Issue #8053). Only the
                                    // selective form is recorded: the IR does not
                                    // distinguish non-selective `using M` (which
                                    // exposes M's exports via getproperty) from
                                    // `import M` (which does not), so registering
                                    // every export for the `None` case would risk
                                    // wrongly exposing `import M` members.
                                }
                            }
                            // Import all exported functions: `using Module`
                            None if !using_import.is_import => {
                                for func_name in module_funcs {
                                    if all_exported
                                        || exports.is_some_and(|e| e.contains(func_name))
                                    {
                                        imported.insert(func_name.clone());
                                    }
                                }
                            }
                            None => {}
                        }
                    }
                }
            }
            module_imports_map.insert(module_path.clone(), imported);
        }
        *module_usings_map = module_usings;
    }

    fn build_inference_engine(&mut self) -> abstract_interp::InferenceEngine {
        let base_function_count = self.base_function_count;
        let opt_main = self.opt_main;
        let all_modules = &self.all_modules;
        let all_functions = &self.all_functions;
        let func_idx_to_parent = &self.func_idx_to_parent;
        let precompiled_base = self.precompiled_base;
        let cached_inference_results = self.cached_inference_results;
        let shared_ctx = &mut self.shared_ctx;

        // Build a map from abstract type name to its parent for converting Struct
        // to AbstractUser. Sourced from `shared_ctx.abstract_types`, which (unlike
        // the bare `program.abstract_types`) also carries abstract types declared
        // inside modules / bundled packages (Issues #7263 / #7265) so a
        // module-local annotation like `f(d::Distribution)` is resolved to the
        // abstract type rather than a concrete `Struct("Distribution")`.
        let abstract_type_parents: HashMap<String, Option<String>> = shared_ctx
            .abstract_types
            .iter()
            .map(|at| (at.name.clone(), at.parent.clone()))
            .collect();

        let _total_functions = all_functions.len();

        // Build a shared inference engine ONCE before the loop.
        // Rebuilding one-shot inference engines inside the loop used to clone all
        // ~5000 functions on every iteration (O(n^2)).
        // This shared engine clones functions once (O(n)) and reuses the return-type cache.
        let clone_with_rename = |(idx, (func, _)): (usize, &(&Function, Option<String>))| {
            let mut func = (*func).clone();
            if let Some(parent) = func_idx_to_parent.get(&idx) {
                func.name = format!("{}#{}", parent, func.name);
            }
            func
        };

        // Issue #6348 / #10114: on the cached-Base path the first
        // `base_function_count` entries of `all_functions` are exactly the
        // prelude functions (the cache is bypassed when a user definition
        // replaces a Base signature), and nested-function renames only apply
        // to inline entries past the Base segment. A background prefetch
        // thread (`cache::begin_warm_start_prefetch`) already did BOTH the
        // clone AND the `function_table`/`ambiguous_functions` insertion work
        // for that prefix (Issue #10114) — `InferenceEngine::add_function`'s
        // per-call work for ~5000 Base+prelude functions used to dominate
        // `compile.build_inference_engine` even after the plain clone was
        // hidden behind the prefetch. Seed the fresh engine from that
        // snapshot directly and only run `add_function` for the (typically
        // small) suffix.
        enum InferenceEngineSeed {
            Prefetched {
                function_table: HashMap<String, Function>,
                ambiguous_functions: HashSet<String>,
                suffix_functions: Vec<Function>,
            },
            Full {
                functions: Vec<Function>,
            },
        }

        let prefetched_base_table =
            if precompiled_base.is_some() && base_function_count <= all_functions.len() {
                cache::take_prefetched_base_function_table(base_function_count)
            } else {
                None
            };

        let engine_seed =
            profile::time(
                "compile.inference_functions_clone",
                || match prefetched_base_table {
                    Some((function_table, ambiguous_functions)) => {
                        let suffix_functions = all_functions
                            .iter()
                            .enumerate()
                            .skip(base_function_count)
                            .map(|(idx, entry)| clone_with_rename((idx, entry)))
                            .collect();
                        InferenceEngineSeed::Prefetched {
                            function_table,
                            ambiguous_functions,
                            suffix_functions,
                        }
                    }
                    None => {
                        let functions = all_functions
                            .iter()
                            .enumerate()
                            .map(|(idx, entry)| clone_with_rename((idx, entry)))
                            .collect();
                        InferenceEngineSeed::Full { functions }
                    }
                },
            );

        let mut inference_global_types = shared_ctx.global_types.clone();
        widen_non_const_globals_for_binding_inference(&opt_main.stmts, &mut inference_global_types);
        for module in all_modules {
            widen_non_const_globals_for_binding_inference(
                &module.body.stmts,
                &mut inference_global_types,
            );
        }
        shared_ctx.inference_global_types = inference_global_types.clone();
        let boundary_idx = opt_main.stmts.iter().position(is_base_user_main_boundary);
        let mut assigned_user_globals = HashSet::new();
        if let Some(idx) = boundary_idx {
            collect_assigned_binding_names(&opt_main.stmts[idx + 1..], &mut assigned_user_globals);
        }
        let mut shadowed_user_globals = assigned_user_globals.clone();
        shadowed_user_globals.extend(self.user_function_names.iter().cloned());
        for name in &shadowed_user_globals {
            inference_global_types.remove(name);
        }

        let mut inference_engine =
            profile::time("compile.build_inference_engine", || match engine_seed {
                InferenceEngineSeed::Prefetched {
                    function_table,
                    ambiguous_functions,
                    suffix_functions,
                } => build_shared_inference_engine_owned_with_prefetched_base(
                    &shared_ctx.struct_table,
                    &inference_global_types,
                    function_table,
                    ambiguous_functions,
                    suffix_functions,
                ),
                InferenceEngineSeed::Full { functions } => build_shared_inference_engine_owned(
                    &shared_ctx.struct_table,
                    &inference_global_types,
                    functions,
                ),
            });
        let has_seeded_inference_results =
            cached_inference_results.is_some_and(|entries| !entries.is_empty());
        profile::time("compile.seed_inference_results", || {
            if let Some(entries) = cached_inference_results {
                inference_engine.seed_return_cache(entries.iter().cloned());
            }
        });

        // Issue #6538: on the cached-Base path, `build_method_tables` below
        // short-circuits every cached Base function (`is_cached_base_function`)
        // without registering its `MethodSig`s into the inference engine, so a
        // user function calling a multi-method Base function got NO inference
        // information from the engine method tables (and `add_function` had
        // already dropped multi-signature names as ambiguous from the function
        // table). Such calls fell through to the tfunc registry and inferred
        // `Any` where the uncached path infers precisely. Seed the engine's
        // method tables wholesale from the cached Base tables — `Arc`-shared
        // method vectors make this O(#tables) — so both compile paths resolve
        // calls through the same method-table snapshot channel. The gate
        // mirrors `is_cached_base_function` in `build_method_tables`.
        if self.precompiled_base.is_some() {
            if let Some(cached_tables) = self.cached_method_tables {
                profile::time("compile.seed_engine_method_tables", || {
                    inference_engine.seed_initial_method_tables(cached_tables.iter());
                    let body_indices: HashSet<usize> = cached_tables
                        .values()
                        .flat_map(|table| table.methods.iter())
                        .filter(|sig| abstract_interp::InferenceEngine::method_sig_needs_body(sig))
                        .map(|sig| sig.global_index)
                        .collect();
                    for global_index in body_indices {
                        if let Some(entry) = all_functions.get(global_index) {
                            let body = clone_with_rename((global_index, entry));
                            inference_engine.add_method_body(global_index, body);
                        }
                    }
                });
            }
        }

        self.abstract_type_parents = abstract_type_parents;
        self.shadowed_user_globals = shadowed_user_globals;
        self.has_seeded_inference_results = has_seeded_inference_results;
        inference_engine
    }

    fn build_method_tables(&mut self, inference_engine: &mut abstract_interp::InferenceEngine) {
        let base_function_count = self.base_function_count;
        let cached_base_len = self.cached_base_len;
        let first_user_function_idx = self.first_user_function_idx;
        let inline_start_idx = self.inline_start_idx;
        let current_input_source_function_indices = repl_current_input_source_function_indices(
            &self.all_functions,
            first_user_function_idx,
            inline_start_idx,
            &self.func_idx_to_parent,
            self.repl_current_function_count,
        );
        let precompiled_base = self.precompiled_base;
        let cached_method_tables = self.cached_method_tables;
        let has_seeded_inference_results = self.has_seeded_inference_results;
        let all_functions = &self.all_functions;
        let root_main_span = self.program.main.span;
        let base_user_main_boundary_start = self
            .program
            .main
            .stmts
            .iter()
            .find(|stmt| is_base_user_main_boundary(stmt))
            .map(|stmt| stmt.span().start);
        let user_main_contains_function_def = self
            .opt_main
            .stmts
            .iter()
            .position(is_base_user_main_boundary)
            .map(|idx| {
                self.opt_main.stmts[idx + 1..]
                    .iter()
                    .any(stmt_contains_function_def)
            })
            .unwrap_or_else(|| block_contains_function_def(self.opt_main));
        let user_defines_methods =
            !self.opt_user_functions.is_empty() || user_main_contains_function_def;
        let func_idx_to_parent = &self.func_idx_to_parent;
        let module_body_scoped_root_indices = &self.module_body_scoped_root_indices;
        let base_cached_extra_function_indices = &self.base_cached_extra_function_indices;
        let module_struct_names = &self.module_struct_names;
        let abstract_type_parents = &self.abstract_type_parents;
        let callable_typeof_aliases = &self.callable_typeof_aliases;
        let toplevel_import_sources = &self.toplevel_import_sources;
        let module_usings_map = &self.module_usings_map;
        let shared_ctx = &mut self.shared_ctx;
        let method_tables = &mut self.method_tables;
        let function_infos = &mut self.function_infos;
        let func_index_map = &mut self.func_index_map;
        let show_methods = &mut self.show_methods;
        let print_methods = &mut self.print_methods;
        let specializable_functions = &mut self.specializable_functions;
        let global_index = &mut self.global_index;
        let mut registered_definition_orders = HashMap::<usize, u64>::new();
        let mut registered_lowering_helper_indices: HashSet<usize> = function_infos
            .iter()
            .enumerate()
            .filter_map(|(index, function)| function.is_lowering_helper.then_some(index))
            .collect();
        // Issue #9189: preloaded-package bytecode cache lookup. `None` with
        // zero cost whenever no preload cache was loaded for this compile
        // (the default while `preload_cache::PRELOAD_PACKAGES` is empty).
        //
        // Issue #9230/#9254: whole-prefix-reuse gate. The cache's captured bodies
        // carry frozen absolute function indices from the closure-layout compile
        // that produced it, so they are only valid when THIS program's non-Base
        // function prefix matches that layout exactly (same functions, same
        // order). The layout spans the FULL non-Base region — package functions,
        // their module closures, AND the trailing lifted Base closures a spliced
        // body can reach (Issue #9254) — so any interposed user `function`, user
        // body closure, or main lifted lambda that shifts that region fails the
        // match and refuses the cache wholesale (fail-safe, never a stale-index
        // dispatch). Programs that are only `using`s + main expressions with no
        // lifted lambda (e.g. `plot([1, 2, 3])`, `plot(sin)`) keep the whole
        // non-Base region identical to generation, so the gate stays active.
        // Issue #9646: the closure layout protects frozen function indices, but
        // spliced package bodies also carry frozen concrete struct type_ids in
        // `NewStruct` operands. A root user struct is assigned before module
        // structs and shifts that separate index space, so fail-safe until the
        // preload cache records/relocates struct layouts too.
        let user_top_level_structs_shift_preload_type_ids =
            self.program.structs.iter().any(|def| {
                def.span.start >= root_main_span.start
                    && def.span.end <= root_main_span.end
                    && def.span.start_line >= root_main_span.start_line
                    && def.span.end_line <= root_main_span.end_line
            });
        let preload_prefix_aligned = match self.preload_closure_layout {
            Some(layout) if !user_top_level_structs_shift_preload_type_ids => {
                all_functions.len() >= base_function_count + layout.len()
                    && all_functions
                        .iter()
                        .skip(base_function_count)
                        .zip(layout.iter())
                        .all(|(entry, want)| entry.1 == want.0 && entry.0.name == want.1)
            }
            None => false,
            Some(_) => false,
        };
        let preload_module_cache = self.preload_module_cache.filter(|_| preload_prefix_aligned);
        let preload_reused = &mut self.preload_reused;
        let cached_base_specializations = precompiled_base
            .filter(|_| cached_method_tables.is_some())
            .filter(|base| !base.specializable_functions.is_empty());
        if let Some(base) = cached_base_specializations {
            debug_assert!(
                specializable_functions.is_empty(),
                "cached Base specializable functions must stay at the front so cached CallSpecialize indices remain valid"
            );
            profile::time("compile.cached_base_specializations_restore", || {
                specializable_functions.extend(base.specializable_functions.iter().cloned());
                for &(fallback_index, spec_index) in &base.runtime_specialization_map {
                    if fallback_index < base_function_count
                        && spec_index < base.specializable_functions.len()
                    {
                        shared_ctx
                            .spec_func_mapping
                            .insert(fallback_index, spec_index);
                    }
                }
            });
        }

        let mut cached_base_fast_count = 0usize;
        let mut cached_base_extra_reused_count = 0usize;
        let mut cached_base_rebuild_count = 0usize;
        let mut non_cached_function_count = 0usize;
        let cached_base_function_info_by_key: HashMap<(String, usize), Vec<usize>> =
            if precompiled_base.is_some() && cached_method_tables.is_some() {
                let mut cached = HashMap::new();
                for (idx, info) in function_infos
                    .iter()
                    .take(cached_base_len)
                    .enumerate()
                    .filter(|(_, info)| info.code_start != info.code_end)
                {
                    let key = (info.name.clone(), info.params.len());
                    cached.entry(key).or_insert_with(Vec::new).push(idx);
                }
                cached
            } else {
                HashMap::new()
            };

        // Issue #9140: when every cached top-level Base function takes the
        // super-fast-path (specializations already restored from the cache, so
        // the loop body below is just an identity push + continue for each of
        // them), pre-fill func_index_map with the identity mapping
        // 0..base_function_count in a single extend and start the loop at the
        // first non-top-level-Base function.
        //
        // Issue #10211: `base_function_count` is only the flat
        // `Program.functions` Base prefix. The Base cache's `FunctionInfo`
        // prefix is wider: it also contains deterministic lifted
        // Base/prelude helpers collected from Base main/function bodies. Those
        // helpers can sit after user functions in `all_functions`, so they
        // cannot be handled by this positional pre-fill; the per-iteration
        // cached-name lookup below maps them back to their cached
        // `FunctionInfo`s exactly. Loaded Base module functions are recorded
        // too, but reused only while the user program defines no methods; user
        // methods can invalidate dispatch metadata inside Base modules (Issue
        // #10782).
        // `cached_base_specializations.is_some()` already implies
        // `cached_method_tables.is_some() && precompiled_base.is_some()` (see its
        // construction above).
        let base_loop_start_idx = if cached_base_specializations.is_some() {
            let n = base_function_count.min(all_functions.len());
            func_index_map.extend(0..n);
            cached_base_fast_count = n;
            n
        } else {
            0
        };

        for (all_funcs_idx, (func, module_path)) in
            all_functions.iter().enumerate().skip(base_loop_start_idx)
        {
            // Issue #10236: true when this function was collected directly
            // from a module-body `let`/`@testset` (a lexically-scoped LOCAL,
            // not a genuine top-level module generic function) — see
            // `module_body_scoped_root_indices`'s doc comment. Computed once
            // per function here since both the method-table registration
            // block and the `FunctionInfo` construction block below need it.
            let is_module_body_scoped_root =
                module_body_scoped_root_indices.contains(&all_funcs_idx);
            let cached_lookup_name = if let Some(parent) = func_idx_to_parent.get(&all_funcs_idx) {
                format!("{}#{}", parent, func.name)
            } else if let Some(module_path) = module_path {
                format!("{}.{}", module_path, func.name)
            } else {
                func.name.clone()
            };
            let cached_lookup_key = (cached_lookup_name, func.params.len());
            // Fast path for cached Base functions that need specialization rebuild:
            // function_infos[all_funcs_idx] already holds params/kwparams/return_type
            // from the cache, method tables are pre-populated, and show_methods are
            // pre-populated. The only remaining work is identity push into func_index_map
            // and specialization registration so cached CallSpecialized instructions
            // resolve. Without this short-circuit, the loop below calls
            // inference_engine.infer_function for every cached Base function and
            // throws the result away, dominating startup
            // (~1.3 s of 1.4 s total for `println(1+1)` on Mac M1).
            let is_cached_base_function = (all_funcs_idx + 1) <= base_function_count
                && cached_method_tables.is_some()
                && precompiled_base.is_some();
            if is_cached_base_function {
                let func_info_idx = all_funcs_idx;
                func_index_map.push(func_info_idx);

                if cached_base_specializations.is_some() {
                    // Unreachable when the pre-fill above ran (those indices were
                    // skipped); kept as a safety net for future guard changes.
                    cached_base_fast_count += 1;
                    continue;
                }
                cached_base_rebuild_count += 1;

                let is_specializable = if let Some(path) = module_path {
                    path != "Core" && !path.starts_with("Core.")
                } else {
                    true
                };
                let specialization_lookup_name =
                    if let Some(parent) = func_idx_to_parent.get(&all_funcs_idx) {
                        format!("{}#{}", parent, func.name)
                    } else if let Some(module_path) = module_path {
                        format!("{}.{}", module_path, func.name)
                    } else {
                        func.name.clone()
                    };
                let has_recorded_closure_captures = shared_ctx
                    .closure_captures
                    .contains_key(&specialization_lookup_name);
                let runtime_specialize =
                    needs_specialization(func) || has_recorded_closure_captures;
                // Issue #5003: also register where/value-parametrized methods so
                // reflection-time inference can find them, but do NOT add them to
                // spec_func_mapping (which drives CallSpecialize emission) unless they
                // truly need runtime specialization — that would bypass dispatch.
                // This whole branch only runs for cached Base functions (see
                // `is_cached_base_function` above), so `is_user_defined` is always
                // `false` here — the Issues #10145/#10264 widening applies only to
                // module/user-authored functions, not the Base corpus.
                let reflection_register = needs_reflection_registration(func, false);
                if is_specializable && (runtime_specialize || reflection_register) {
                    let spec_idx = specializable_functions.len();
                    specializable_functions.push(SpecializableFunction {
                        ir: std::sync::Arc::new((*func).clone()),
                        name: func.name.clone(),
                        fallback_index: func_info_idx,
                    });
                    if runtime_specialize {
                        shared_ctx.spec_func_mapping.insert(func_info_idx, spec_idx);
                    }
                }
                continue;
            }

            // Issue #10211: reuse deterministic lifted Base/prelude helpers that
            // the Base cache already compiled but that no longer occupy
            // positional indices `< base_function_count` in this run's
            // `all_functions` order. This happens for helpers collected from
            // Base main/function bodies: user top-level functions are inserted
            // before the trailing inline block, so the helper's all-functions
            // index shifts even though its cached `FunctionInfo` (and bytecode)
            // is still valid. Match by the same canonical FunctionInfo name the
            // cached compile produced (`parent#name`, `Module.name`, or bare
            // name). Prefer the recorded provenance set. Positional exceptions
            // are exact module-qualified matches (`Module.name`) and
            // pre-user-main bare functions, both only while the user main does
            // not define or extend methods. Do not reuse cached module
            // bytecode once user methods exist: Base modules such as `Order`
            // can dispatch back to those methods (Issue #10782). Also never
            // use a broad bare positional match, because user/package
            // generated names like `operator` can otherwise alias unrelated
            // cached Base entries.
            let positional_cached_func_info_idx =
                if precompiled_base.is_some() && all_funcs_idx < cached_base_len {
                    function_infos.get(all_funcs_idx).and_then(|info| {
                        (info.code_start != info.code_end
                            && info.name == cached_lookup_key.0
                            && info.params.len() == cached_lookup_key.1)
                            .then_some(all_funcs_idx)
                    })
                } else {
                    None
                };
            let positional_cached_pre_user_main_extra = positional_cached_func_info_idx.is_some()
                && module_path.is_none()
                && base_user_main_boundary_start.is_some_and(|boundary| func.span.start < boundary);
            let positional_cached_module_extra =
                positional_cached_func_info_idx.is_some() && module_path.is_some();
            let can_reuse_cached_extra = (!user_defines_methods
                || self.repl_append_only_new_generics)
                && (base_cached_extra_function_indices.contains(&all_funcs_idx)
                    || positional_cached_pre_user_main_extra
                    || positional_cached_module_extra);
            if can_reuse_cached_extra {
                let cached_func_info_idx = positional_cached_func_info_idx.or_else(|| {
                    cached_base_function_info_by_key
                        .get(&cached_lookup_key)
                        .and_then(|indices| match indices.as_slice() {
                            [idx] => Some(*idx),
                            _ => {
                                let matching: Vec<usize> = indices
                                    .iter()
                                    .copied()
                                    .filter(|idx| {
                                        function_infos.get(*idx).is_some_and(|info| {
                                            info.kwparams.len() == func.kwparams.len()
                                                && info.def_line == func.span.start_line as u32
                                        })
                                    })
                                    .collect();
                                match matching.as_slice() {
                                    [idx] => Some(*idx),
                                    _ => None,
                                }
                            }
                        })
                });
                if let Some(func_info_idx) = cached_func_info_idx {
                    func_index_map.push(func_info_idx);
                    cached_base_fast_count += 1;
                    cached_base_extra_reused_count += 1;
                    continue;
                }
            }
            non_cached_function_count += 1;

            // Build params early (needed for both method tables and show methods)
            // For module functions, qualify struct type names to match the qualified struct instances.
            // Also convert Struct types to AbstractUser when the type is actually an abstract type.
            let params: Vec<(String, JuliaType)> = func
                .params
                .iter()
                .map(|p| {
                    let ty = p.effective_type();
                    let qualified_ty =
                        qualify_type_for_module(ty, module_path.as_ref(), module_struct_names);
                    let resolved_ty = resolve_abstract_type(qualified_ty, abstract_type_parents);
                    // Resolve type aliases (Issue #2527): const IntWrapper = Wrapper{Int64}
                    let alias_resolved = resolve_type_alias_in_module_scope(
                        resolved_ty,
                        module_path.as_ref(),
                        &shared_ctx.type_aliases,
                    );
                    (p.name.clone(), alias_resolved)
                })
                .collect();

            // Issue #9189: preloaded-package bytecode cache lookup. Reuses
            // `params` (just resolved above) directly instead of re-deriving
            // a matching key from raw IR — `param_julia_types`/`FunctionInfo.name`
            // are qualified/resolved forms that a from-scratch IR-only key
            // builder cannot safely replicate (see
            // `signature_key_for_resolved_params`'s doc comment for the two
            // bugs this avoided: module-qualified names, and
            // qualify_type_for_module/resolve_abstract_type/resolve_type_alias
            // resolution on parameter types). `None`/miss whenever no preload
            // cache was loaded, this function's module isn't preload-cache-listed,
            // or this exact name+signature wasn't captured — all fall through
            // to the ordinary compile path below, unaffected.
            let preload_hit = module_path.as_ref().and_then(|path| {
                preload_module_cache
                    .and_then(|cache| cache.get(path))
                    .and_then(|cached_module| {
                        let key = super::preload_cache::signature_key_for_resolved_params(
                            &func.name, &params,
                        );
                        cached_module.functions.get(&key).cloned()
                    })
            });

            // Skip Base functions if we're using cached method tables (Option A optimization)
            // Base methods are already in the cached method tables
            // When using cache, global_index starts at base_function_count, so we use loop counter instead
            // Note: all_funcs_idx is 0-indexed, so we use <= to match 1-indexed behavior
            let is_base_function = (all_funcs_idx + 1) <= base_function_count;

            // Build vm_params, vm_kwparams, and return_type (needed for FunctionInfo)
            let vm_params: Vec<(String, ValueType)> = params
                .iter()
                .map(|(name, jt)| {
                    (
                        name.clone(),
                        julia_type_to_value_type_scoped(
                            jt,
                            &shared_ctx.struct_table,
                            is_base_function,
                        ),
                    )
                })
                .collect();

            let vm_kwparams: Vec<KwParamInfo> = func
                .kwparams
                .iter()
                .map(|kw| {
                    let required = is_required_kwarg(&kw.default);
                    // Normalize keyword annotations through the same
                    // qualification / abstract / alias pipeline as positional
                    // parameters above. Keeping the raw alias spelling in
                    // `KwParamInfo.declared_type` made a valid mutable
                    // user-struct value compare against the alias name rather
                    // than its target (Issues #11024, #11135).
                    let declared_type = kw.type_annotation.clone().map(|ty| {
                        let qualified =
                            qualify_type_for_module(ty, module_path.as_ref(), module_struct_names);
                        let resolved = resolve_abstract_type(qualified, abstract_type_parents);
                        resolve_type_alias_in_module_scope(
                            resolved,
                            module_path.as_ref(),
                            &shared_ctx.type_aliases,
                        )
                    });
                    // For varargs kwargs (kwargs...), type is always Pairs (Julia's Base.Pairs)
                    // For required kwargs, use type annotation if available; otherwise use Any
                    // Optional kwargs remain Any in the compiled body.
                    let ty = if kw.is_varargs {
                        ValueType::Pairs
                    } else if required {
                        declared_type
                            .as_ref()
                            .map(|jt| {
                                julia_type_to_value_type_scoped(
                                    jt,
                                    &shared_ctx.struct_table,
                                    is_base_function,
                                )
                            })
                            .unwrap_or(ValueType::Any)
                    } else if kw.body_evaluated_default {
                        // The default is re-evaluated inside the body (Issue #5121).
                        // The kwsorter binds the `Undef` sentinel to the slot for an
                        // omitted keyword, and the body prologue overwrites it with the
                        // real default (any type), so the slot must be `Any`.
                        ValueType::Any
                    } else {
                        // Every OPTIONAL kwarg's slot is `Any`: the default's type must
                        // not constrain the slot (Issue #5425, generalizing #5416), and
                        // an ANNOTATED optional kwarg (`x::Real = 1`, Issue #11024) must
                        // not either — its declared type is an ASSERTION checked against
                        // the supplied value at bind time (`check_kwarg_declared_type`),
                        // not a slot type: `Real` has no faithful `ValueType`, and
                        // freezing the slot to the default's inferred type would reject
                        // a perfectly valid `h(x = 2.5)`.
                        //
                        // (Before #11024 carried keyword annotations through lowering,
                        // `is_unannotated_optional_kwparam` was trivially true here and
                        // the `infer_default_type` fallback was unreachable for keyword
                        // parameters, so this is the same slot typing as before.)
                        ValueType::Any
                    };
                    KwParamInfo {
                        name: kw.name.clone(),
                        // For body-evaluated defaults the kwsorter must bind the
                        // `Undef` sentinel so the prologue's `kw === Undef` guard
                        // fires; the real default lives in the body (Issue #5121).
                        default: if kw.body_evaluated_default {
                            Value::Undef
                        } else {
                            eval_literal_default(&kw.default)
                        },
                        default_expr: if required || kw.is_varargs || kw.body_evaluated_default {
                            None
                        } else {
                            Some(kw.default.clone())
                        },
                        ty,
                        // Issue #11024: the DECLARED keyword type, asserted against every
                        // supplied value at bind time (`check_kwarg_declared_type`).
                        declared_type,
                        slot: 0,
                        required,
                        is_varargs: kw.is_varargs,
                    }
                })
                .collect();

            // Use declared return type if available, otherwise infer from function body
            // Using the shared inference engine (created once before the loop) for
            // abstract interpretation. The engine caches return types across calls.
            let (mut return_type, return_julia_type) =
                if let Some(ref declared_rt) = func.return_type {
                    let vt = julia_type_to_value_type_scoped(
                        declared_rt,
                        &shared_ctx.struct_table,
                        is_base_function,
                    );
                    // Declared return types already carry parametric info via JuliaType
                    let jt = if matches!(declared_rt, JuliaType::TupleOf(_)) {
                        Some(declared_rt.clone())
                    } else {
                        None
                    };
                    (vt, jt)
                } else if should_defer_module_return_inference(
                    func,
                    module_path.as_ref(),
                    is_base_function,
                ) {
                    // Package/module methods without declared return types dominate
                    // `using Package` startup when every method is inferred eagerly.
                    // Keep dispatch safe by recording an `Any` snapshot, while
                    // preserving cheap syntactic type-parameter/direct-parameter
                    // snapshots used by reflection and datatype-return call sites
                    // (Issue #8463).
                    let type_param_jt = type_parameter_return_snapshot(func);
                    let jt = type_param_jt
                        .clone()
                        .or_else(|| direct_parameter_return_snapshot(func));
                    let vt = if type_param_jt.is_some() {
                        ValueType::DataType
                    } else {
                        ValueType::Any
                    };
                    (vt, jt)
                } else {
                    let rt = inference_engine.infer_function(func);
                    let inferred_vt = bridge::lattice_to_value_type(&rt);
                    // Extract parametric tuple type that ValueType::Tuple would lose (Issue #2317)
                    let type_param_jt = type_parameter_return_snapshot(func);
                    let jt = type_param_jt
                        .clone()
                        .or_else(|| direct_parameter_return_snapshot(func))
                        .or_else(|| bridge::lattice_to_parametric_julia_type(&rt));
                    let mut vt = if jt.is_some() {
                        if type_param_jt.is_some() {
                            ValueType::DataType
                        } else {
                            inferred_vt
                        }
                    } else {
                        inferred_vt
                    };
                    if has_abstract_numeric_param(&params) && is_concrete_numeric_return_type(&vt) {
                        // Abstract numeric parameters (`x::Number`, `x::Real`, ...)
                        // accept BigInt/BigFloat and primitive numeric values. A single
                        // concrete numeric return type inferred from the method body is
                        // therefore a storage hazard for VM calls: callers would emit a
                        // typed StoreSlot and reject valid runtime results (Issue #4337).
                        vt = ValueType::Any;
                    };
                    if returns_untyped_param_power_value(func) {
                        // `^` over an untyped parameter must preserve the runtime
                        // `DynamicPow` result (`Int^Int -> Int`, negative exponents ->
                        // Float64) instead of pinning the single compiled body to F64
                        // (Issue #5608).
                        vt = ValueType::Any;
                    }
                    if matches!(vt, ValueType::Nothing)
                        && directly_returns_unannotated_optional_kwparam(func)
                    {
                        // A `nothing`-default kwarg returned directly must not pin the
                        // function's snapshot return type to the `Nothing` singleton
                        // (Issue #5416). Note we keep a *non-`Nothing`* concrete snapshot
                        // (e.g. `Int64` for `g(; n = 0) = n`) intact so reflection stays
                        // precise; the compiled-body / call-site widening for those is
                        // applied separately (Issue #5425).
                        vt = ValueType::Any;
                    }
                    (vt, jt)
                };
            if func.name == "Dict" || func.name.starts_with("Dict{") {
                // Public Dict constructors now return the pure-Julia
                // `Dict{K,V}` struct. Do not compile those method bodies with
                // the legacy `ValueType::Dict` carrier / ReturnDict path
                // (Issue #6619).
                return_type = ValueType::Any;
            }
            if let Some(JuliaType::Struct(name)) = &return_julia_type {
                let base = name.rsplit('.').next().unwrap_or(name);
                if base.split('{').next() == Some("Dict") {
                    let compact_name = compact_type_name(name);
                    if let Some(info) = shared_ctx
                        .struct_table
                        .resolve(name)
                        .map(|(_, info)| info)
                        .or_else(|| {
                            shared_ctx
                                .struct_table
                                .iter()
                                .find(|(struct_name, _)| {
                                    compact_type_name(struct_name) == compact_name
                                })
                                .map(|(_, info)| info)
                        })
                    {
                        return_type = ValueType::Struct(info.type_id);
                    }
                }
            }
            if func.name == "copy" || func.name == "Base.copy" {
                // Mirrors tfunc_copy's #5867 guard: the current `copy(::Dict)`
                // implementation is a legacy/migration surface and must not
                // be compiled with ReturnDict when public Dict() now creates a
                // struct-backed value (Issue #6619).
                return_type = ValueType::Any;
            }

            let skip_method_table_update = is_base_function && cached_method_tables.is_some();
            // When using cache, skip function_infos.push() for Base functions (already in cache)
            let skip_function_info_push = is_base_function && precompiled_base.is_some();
            let is_runtime_eval_function = func.is_runtime_eval;

            // Detect varargs parameter early (needed for both MethodSig and FunctionInfo)
            let vararg_param_index = func.params.iter().position(|p| p.is_varargs);
            // For Vararg{T, N}: extract fixed count N (Issue #2525)
            let vararg_fixed_count = func
                .params
                .iter()
                .find(|p| p.is_varargs)
                .and_then(|p| p.vararg_count);

            if is_runtime_eval_function {
                shared_ctx
                    .runtime_eval_function_names
                    .insert(func.name.clone());
                shared_ctx
                    .runtime_eval_function_indices
                    .insert(*global_index);
            }

            if !skip_method_table_update && !is_runtime_eval_function {
                let is_lowering_helper =
                    crate::compile::ir_inline::is_markerless_lowered_function(func);
                if is_lowering_helper {
                    registered_lowering_helper_indices.insert(*global_index);
                }
                if !is_base_function {
                    // Issue #7643: user-written Base extensions such as
                    // `import Base: ==; ==(::S, ::S) = ...` need the same IR
                    // metadata as other user methods so dynamic dispatch
                    // candidate builders can see their declared argument types.
                    // Only real Base/prelude functions are excluded here.
                    shared_ctx
                        .function_ir_by_global_index
                        .insert(*global_index, (*func).clone());
                }
                // A nested (inner) function is lexically scoped to its parent, so
                // register it under its qualified `parent#name` table ONLY — never
                // the bare short name. `function_infos`/`function_indices` already
                // key nested functions by this qualified name. Sharing the bare
                // short-name table with a same-named GLOBAL would let the inner
                // definition's signature DEDUP-REPLACE the global's method
                // (`MethodTable::add_method` dedups by signature), so a value
                // reference to the global (`f = g; f()`) — which resolves via the
                // bare table — would pick up the inner function's body instead of
                // the global's (Issue #8105). The module/import/typeof aliases below
                // only apply to top-level / module functions, never inner ones.
                let nested_qualified_name = func_idx_to_parent
                    .get(&all_funcs_idx)
                    .map(|parent| format!("{}#{}", parent, func.name));
                // Issue #10236: a function collected directly from a
                // module-body `let`/`@testset` is a LEXICALLY-SCOPED LOCAL of
                // that block, not a genuine top-level module generic function
                // (see the `module_body_scoped_root_indices` doc comment) —
                // even though it has no `func_idx_to_parent` entry (so
                // `nested_qualified_name` is `None`, like a real top-level
                // module function) and so still receives the SAME bare +
                // qualified method-table registration below (needed for
                // `module_owned_function_table_name`'s own-module compile-time
                // redirect, Issue #7575, which requires BOTH the bare and the
                // qualified table to exist). What changes for it is
                // `FunctionInfo::suppress_short_name_alias` (set below), which
                // keeps the RUNTIME `function_name_index` (used by
                // `Value::Closure`/`Value::Function` dynamic dispatch, a
                // separate index from `method_tables`) from ALSO exposing it
                // under the bare short name — that runtime alias is what let
                // it dedup-collide with an unrelated same-named `let`-root
                // helper from ANOTHER module or from Main, silently routing
                // one scope's closure call to the OTHER scope's body
                // (`is_module_body_scoped_root`, computed once at the top of
                // this loop, drives `FunctionInfo::suppress_short_name_alias`
                // below instead).
                let mut table_names = vec![nested_qualified_name
                    .clone()
                    .unwrap_or_else(|| func.name.clone())];
                // A nested ANONYMOUS function (compiler-generated `__lambda_*` /
                // `__do_block_*`) carries a unique name that cannot collide with a
                // user global, so the qualified-table-only restriction above does
                // not apply to it. It must ALSO stay in the bare short-name table:
                // the higher-order-function return-type specialization resolves the
                // lambda by its bare name, and dropping that registration broke
                // `reduce`/`mapreduce` result-type inference (Issue #5094 regression
                // from #8105; fixed #8129).
                if nested_qualified_name.is_some()
                    && crate::compile::ir_inline::is_markerless_lowered_function(func)
                {
                    table_names.push(func.name.clone());
                }
                if nested_qualified_name.is_none() {
                    if let Some(short_name) = func.name.strip_prefix("Base.") {
                        table_names.push(short_name.to_string());
                    } else if !is_base_function {
                        // A user method explicitly defined on another module's function
                        // (`function Inner.f(...)`) carries a module-qualified name. Also
                        // register it under the bare function table (`f`) so the
                        // unqualified `f(2.0)` brought in by `using .Inner` dispatches
                        // across the module-owned methods, while the qualified
                        // `Inner.f(2.0)` resolves via `func.name` above (Issue #8052).
                        // Only user (non-Base) qualified names get this bare alias so
                        // stdlib `Core.*`/`Base.*` names are unaffected.
                        if let Some((_, bare)) = func.name.rsplit_once('.') {
                            table_names.push(bare.to_string());
                        } else if module_path.is_none() {
                            // A top-level `function f(...)` whose bare name `f` was
                            // selectively imported (`import M: f`) extends `M.f`: also
                            // register the method under the qualified table so a later
                            // `M.f(2.0)` sees it, matching Julia (Issue #8052).
                            if let Some(sources) = toplevel_import_sources.get(&func.name) {
                                for src in sources {
                                    table_names.push(format!("{}.{}", src, func.name));
                                }
                            }
                        }
                    }
                    add_callable_typeof_method_table_aliases(
                        &func.name,
                        callable_typeof_aliases,
                        &mut table_names,
                    );
                    if let Some(module_path) = module_path {
                        table_names.push(format!("{}.{}", module_path, func.name));
                    }
                }
                // Issue #5425 / #5466: a function that returns an unannotated optional
                // kwarg — directly (`g(; n = 0) = n`) or derived through a computation
                // (`g2(; n = 0) = n + 1`) — returns whatever the caller passes for that
                // kwarg, so its *dispatch* return type must be `Any`. Every compile-time
                // call-type inference (binary-op operand typing, discard/assign stores,
                // call-result typing) reads `MethodSig.return_type`; a concrete
                // default-derived type (e.g. `Int64`) would drive a typed
                // comparison/store that rejects a differently-typed passed value.
                // `FunctionInfo.return_type` (set below) stays precise so reflection
                // (`Base.infer_return_type`) keeps the omitted-kwarg signature's type.
                let method_return_type = if returns_unannotated_optional_kwparam_value(func)
                    || returns_untyped_param_power_value(func)
                {
                    ValueType::Any
                } else {
                    return_type.clone()
                };
                let normalized_type_params = shared_ctx.expand_type_param_bounds(&func.type_params);
                let is_top_level_user_function = all_funcs_idx >= first_user_function_idx
                    && all_funcs_idx < inline_start_idx
                    && module_path.is_none()
                    && nested_qualified_name.is_none();
                let is_current_input_top_level_user_function = is_top_level_user_function
                    && current_input_source_function_indices
                        .as_ref()
                        .is_none_or(|indices| indices.contains(&all_funcs_idx))
                    // Lowered arrows/do-blocks are callable values created by
                    // their containing expression, not Julia-visible generic
                    // definitions. They must be callable inside that same
                    // statement instead of waiting for a later top-level
                    // source-order marker (Issue #11477).
                    && !crate::compile::ir_inline::is_markerless_lowered_function(func);
                let is_source_ordered_inline_function = all_funcs_idx >= inline_start_idx
                    && module_path.is_none()
                    && nested_qualified_name.is_none()
                    && !crate::compile::ir_inline::is_markerless_lowered_function(func);
                // Hoisted top-level functions disappear from `program.main`, so
                // its span cannot prove that a current REPL input function is
                // root-source (definition-only and trailing definitions are the
                // canonical counterexamples). The structural current-input
                // source-index set is authoritative for those functions. Keep
                // the span check for inline/source-collected functions outside
                // that set (Issues #9784/#11477).
                let is_structurally_current_repl_function =
                    self.repl_current_function_count.is_some()
                        && is_current_input_top_level_user_function;
                let is_root_source_function = is_structurally_current_repl_function
                    || (func.span.start >= root_main_span.start
                        && func.span.end <= root_main_span.end
                        && func.span.start_line >= root_main_span.start_line
                        && func.span.end_line <= root_main_span.end_line);
                if is_current_input_top_level_user_function && is_root_source_function {
                    shared_ctx
                        .source_world_function_names
                        .insert(func.name.clone());
                }
                let source_visibility_start = if (is_current_input_top_level_user_function
                    || is_source_ordered_inline_function)
                    && is_root_source_function
                {
                    Some(func.span.start)
                } else {
                    None
                };
                for table_name in table_names {
                    if !is_lowering_helper {
                        if let Some(existing) = method_tables.get_mut(&table_name) {
                            let source_only = existing
                                .methods
                                .iter()
                                .filter(|method| {
                                    !registered_lowering_helper_indices
                                        .contains(&method.global_index)
                                })
                                .cloned()
                                .collect();
                            *existing = existing.clone_with_methods_for_compile(source_only);
                        }
                    }
                    if let Some(existing) = method_tables.get(&table_name) {
                        let recorded = shared_ctx
                            .source_ordered_method_sigs
                            .entry(table_name.clone())
                            .or_default();
                        let recorded_indices = recorded
                            .iter()
                            .map(|entry| entry.sig.global_index)
                            .collect::<std::collections::HashSet<_>>();
                        recorded.extend(
                            existing
                                .methods
                                .iter()
                                .filter(|sig| !recorded_indices.contains(&sig.global_index))
                                .cloned()
                                .map(|sig| super::context::SourceOrderedMethodSig {
                                    sig,
                                    visible_from_source_start: None,
                                }),
                        );
                    }
                    // Issue #8079: a user (non-Base) method whose bare name
                    // collides with a Base library function REPLACES the
                    // same-signature base method in the shared short-name table
                    // (`MethodTable::add_method` dedups by signature). An explicit
                    // `Base.<name>(...)` call would then re-dispatch to the user
                    // shadow instead of Base, which self-recurses when the shadow
                    // forwards to `Base.<name>` (e.g. NaNMath.log2 → Base.log2 →
                    // NaNMath.log2 → …) and overflows the call stack. Snapshot the
                    // bare table's base methods *before* adding the user method so
                    // that, if the add actually replaces a base method (only when
                    // the signatures collide — a typed base `log(::Float64)` is
                    // untouched by an untyped `log(::Any)` shadow), they can be
                    // preserved under a dedicated `Base.<name>` table for the
                    // qualified call to dispatch through.
                    let preserve_candidate: Option<Vec<MethodSig>> = if !is_base_function
                        && !table_name.contains('.')
                        && !method_tables.contains_key(&format!("Base.{}", table_name))
                    {
                        method_tables.get(&table_name).map(|existing| {
                            existing
                                .methods
                                .iter()
                                .filter(|m| m.is_base_program_method(base_function_count))
                                .cloned()
                                .collect::<Vec<_>>()
                        })
                    } else {
                        None
                    };
                    let base_before = preserve_candidate.as_ref().map_or(0, |v| v.len());
                    registered_definition_orders.insert(*global_index, func.span.definition_order);
                    let (sig, keep_in_method_table) = {
                        let table = method_tables
                            .entry(table_name.clone())
                            .or_insert_with(|| MethodTable::new(table_name.clone()));

                        let method_index = table.methods.len();
                        let sig = MethodSig::from_julia_projections(
                            method_index,
                            *global_index,
                            params.clone(),
                            method_return_type.clone(),
                            return_julia_type.clone(),
                            func.is_base_extension,
                            normalized_type_params.clone(),
                            vararg_param_index,
                            vararg_fixed_count,
                        );
                        // Definition-bearing fragments are collected by ownership
                        // (dependencies before their parent module), which need not
                        // be Julia evaluation order. If a later same-signature
                        // method is already present, keep it authoritative instead
                        // of letting this earlier method replace it merely because
                        // collection visited the parent last (Issues #11036,
                        // #11128). Zero is legacy cache metadata and deliberately
                        // falls back to collection order.
                        let has_source_method = table.methods.iter().any(|method| {
                            !registered_lowering_helper_indices.contains(&method.global_index)
                        });
                        let keep = if is_lowering_helper {
                            !has_source_method
                        } else {
                            table.ordinary_method_with_same_signature(&sig).is_none_or(
                                |existing_index| {
                                    registered_lowering_helper_indices.contains(&existing_index)
                                        || registered_definition_orders
                                            .get(&existing_index)
                                            .is_none_or(|existing_order| {
                                                func.span.definition_order == 0
                                                    || *existing_order == 0
                                                    || *existing_order <= func.span.definition_order
                                            })
                                },
                            )
                        };
                        if keep {
                            table.add_method(sig.clone());
                        }
                        (sig, keep)
                    };
                    if !is_lowering_helper {
                        shared_ctx
                            .source_ordered_method_sigs
                            .entry(table_name.clone())
                            .or_default()
                            .push(super::context::SourceOrderedMethodSig {
                                sig: sig.clone(),
                                visible_from_source_start: source_visibility_start,
                            });
                    }
                    if keep_in_method_table {
                        if has_seeded_inference_results && !is_base_function {
                            inference_engine.add_method(table_name.clone(), sig.clone());
                        } else {
                            inference_engine.add_initial_method(table_name.clone(), sig.clone());
                        }
                    }
                    if abstract_interp::InferenceEngine::method_sig_needs_body(&sig) {
                        let mut body = (*func).clone();
                        body.name = table_name.clone();
                        inference_engine.add_method_body(*global_index, body);
                    }

                    // The user method genuinely clobbered a base method iff the
                    // bare table now holds fewer base-program methods than before.
                    // Preserve the pre-clobber base methods under `Base.<name>`.
                    if let Some(base_methods) = preserve_candidate {
                        if base_before > 0 {
                            let base_after = method_tables.get(&table_name).map_or(0, |t| {
                                t.methods
                                    .iter()
                                    .filter(|m| m.is_base_program_method(base_function_count))
                                    .count()
                            });
                            if base_after < base_before {
                                let qualified_base = format!("Base.{}", table_name);
                                let mut snapshot = MethodTable::new(qualified_base.clone());
                                snapshot.set_base_function_count(base_function_count);
                                for m in base_methods {
                                    snapshot.add_method(m.clone());
                                    inference_engine.add_initial_method(qualified_base.clone(), m);
                                }
                                method_tables.insert(qualified_base, snapshot);
                            }
                        }
                    }
                }
            }

            // Detect show methods: function Base.show(io::IO, x::SomeStruct)
            // Also detect show methods defined within base library files (e.g., io.jl)
            // Skip for cached Base functions — their show_methods are pre-populated from cache (Issue #2489)
            let is_show_name = func.name == "show" || func.name.rsplit('.').next() == Some("show");
            let extends_base_show = func.is_base_extension
                || is_base_function
                || module_imports_base_symbol(module_path.as_ref(), module_usings_map, "show");
            if !skip_function_info_push && extends_base_show && is_show_name && params.len() >= 2 {
                // First param must be IO type
                if param_type_is_display_io(&params[0].1) {
                    let mut register_show_type_name = |type_name: &str| {
                        // Register under the exact name as written in the signature.
                        show_methods.push(ShowMethodEntry {
                            type_name: type_name.to_string(),
                            func_index: *global_index,
                        });
                        // For a parametric signature such as
                        // `show(io::IO, b::Box{T}) where T`, the second param's
                        // JuliaType is `Struct("Box{T}")`, carrying the type-var name
                        // in the braces. The runtime lookup (`user_show_method_for`)
                        // keys on the value's concrete struct name (e.g.
                        // "Box{Int64}") and only falls back to the bare base name
                        // ("Box"), never to the typevar form. Also register the bare
                        // base name so parametric `where T` show methods are found,
                        // only when the signature is genuinely generic over a
                        // where-clause type variable. A concrete instantiation
                        // such as `show(io::IO, ::Box{Int64})` must not register
                        // the bare `Box` family key; otherwise it over-applies
                        // to `Box{Float64}` and every other instantiation
                        // (Issue #9456).
                        if let Some(brace_idx) = type_name.find('{') {
                            let params_text = &type_name[brace_idx + 1..];
                            let mentions_where_type_param = func.type_params.iter().any(|tp| {
                                params_text
                                    .split(|ch: char| !(ch.is_alphanumeric() || ch == '_'))
                                    .any(|token| token == tp.name)
                            });
                            if mentions_where_type_param {
                                let base_name = type_name[..brace_idx].to_string();
                                show_methods.push(ShowMethodEntry {
                                    type_name: base_name,
                                    func_index: *global_index,
                                });
                            }
                        }
                    };
                    let mut register_show_type = |ty: &JuliaType| match ty {
                        JuliaType::Struct(type_name) | JuliaType::AbstractUser(type_name, _) => {
                            register_show_type_name(type_name);
                        }
                        JuliaType::Union(members) => {
                            for member in members {
                                if let JuliaType::Struct(type_name)
                                | JuliaType::AbstractUser(type_name, _) = member
                                {
                                    register_show_type_name(type_name);
                                }
                            }
                        }
                        // Register abstract type annotations that can cover user
                        // structs through declared parents. Intentionally skip
                        // `Any`: the generic `show(io, x)` fallback should not
                        // force every struct through the show-method shortcut.
                        JuliaType::Number
                        | JuliaType::Real
                        | JuliaType::Integer
                        | JuliaType::Signed
                        | JuliaType::Unsigned
                        | JuliaType::AbstractFloat
                        | JuliaType::AbstractString
                        | JuliaType::AbstractChar
                        | JuliaType::AbstractArray
                        | JuliaType::AbstractRange
                        | JuliaType::Function
                        | JuliaType::IO => {
                            let type_name = ty.name();
                            register_show_type_name(type_name.as_ref());
                        }
                        _ => {}
                    };
                    let ty = &params[1].1;
                    register_show_type(ty);
                }
            }

            // Detect print methods: function Base.print(io::IO, x::SomeStruct).
            // Print paths prefer these methods and fall back to show methods
            // for user structs that only define show (Issue #9460).
            let is_print_name =
                func.name == "print" || func.name.rsplit('.').next() == Some("print");
            let extends_base_print = func.is_base_extension
                || is_base_function
                || module_imports_base_symbol(module_path.as_ref(), module_usings_map, "print");
            if !skip_function_info_push
                && extends_base_print
                && is_print_name
                && params.len() >= 2
                && param_type_is_display_io(&params[0].1)
            {
                let mut register_print_type_name = |type_name: &str| {
                    print_methods.push(ShowMethodEntry {
                        type_name: type_name.to_string(),
                        func_index: *global_index,
                    });
                    if let Some(brace_idx) = type_name.find('{') {
                        let params_text = &type_name[brace_idx + 1..];
                        let mentions_where_type_param = func.type_params.iter().any(|tp| {
                            params_text
                                .split(|ch: char| !(ch.is_alphanumeric() || ch == '_'))
                                .any(|token| token == tp.name)
                        });
                        if mentions_where_type_param {
                            let base_name = type_name[..brace_idx].to_string();
                            print_methods.push(ShowMethodEntry {
                                type_name: base_name,
                                func_index: *global_index,
                            });
                        }
                    }
                };
                let mut register_print_type = |ty: &JuliaType| match ty {
                    JuliaType::Struct(type_name) | JuliaType::AbstractUser(type_name, _) => {
                        register_print_type_name(type_name);
                    }
                    JuliaType::Union(members) => {
                        for member in members {
                            if let JuliaType::Struct(type_name)
                            | JuliaType::AbstractUser(type_name, _) = member
                            {
                                register_print_type_name(type_name);
                            }
                        }
                    }
                    JuliaType::Number
                    | JuliaType::Real
                    | JuliaType::Integer
                    | JuliaType::Signed
                    | JuliaType::Unsigned
                    | JuliaType::AbstractFloat
                    | JuliaType::AbstractString
                    | JuliaType::AbstractChar
                    | JuliaType::AbstractArray
                    | JuliaType::AbstractRange
                    | JuliaType::Function
                    | JuliaType::IO => {
                        let type_name = ty.name();
                        register_print_type_name(type_name.as_ref());
                    }
                    _ => {}
                };
                let ty = &params[1].1;
                register_print_type(ty);
            }

            // Build func_index_map and function_infos
            // When using cache, Base functions are already in function_infos (from cache clone)
            let func_info_idx = if skip_function_info_push {
                // Base function using cache: identity mapping (index in all_functions = index in function_infos)
                // all_funcs_idx is 0-indexed, same as function_infos
                func_index_map.push(all_funcs_idx);
                all_funcs_idx
            } else {
                // User function or no cache: push to function_infos, map to new index
                let idx = function_infos.len();
                func_index_map.push(idx);

                // Preserve original JuliaTypes for type parameter binding
                let param_julia_types: Vec<JuliaType> =
                    params.iter().map(|(_, jt)| jt.clone()).collect();

                // Retain representative reflection metadata from the leading
                // @inline/@noinline/@propagate_inbounds/@constprop/@nospecialize(infer)/
                // @assume_effects markers (Issues #4977/#4978/#4979/#4980/#4981/
                // #4983/#4984).
                let reflection_meta = function_reflection_meta(func);

                // For nested functions, use qualified name (parent#nested) to avoid collisions
                // when multiple parent functions have nested functions with the same name (Issue #1743)
                let function_name = if let Some(parent) = func_idx_to_parent.get(&all_funcs_idx) {
                    format!("{}#{}", parent, func.name)
                } else if let Some(module_path) = module_path {
                    format!("{}.{}", module_path, func.name)
                } else {
                    func.name.clone()
                };
                let is_top_level_user_function_info = all_funcs_idx >= first_user_function_idx
                    && all_funcs_idx < inline_start_idx
                    && module_path.is_none()
                    && !func_idx_to_parent.contains_key(&all_funcs_idx);
                let is_current_input_top_level_user_function_info = is_top_level_user_function_info
                    && current_input_source_function_indices
                        .as_ref()
                        .is_none_or(|indices| indices.contains(&all_funcs_idx))
                    && !crate::compile::ir_inline::is_markerless_lowered_function(func);
                let is_source_ordered_inline_function_info = all_funcs_idx >= inline_start_idx
                    && module_path.is_none()
                    && !func_idx_to_parent.contains_key(&all_funcs_idx)
                    && !crate::compile::ir_inline::is_markerless_lowered_function(func);
                let is_root_source_function_info = func.span.start >= root_main_span.start
                    && func.span.end <= root_main_span.end
                    && func.span.start_line >= root_main_span.start_line
                    && func.span.end_line <= root_main_span.end_line;

                let is_lowering_helper =
                    crate::compile::ir_inline::is_markerless_lowered_function(func);
                function_infos.push(std::rc::Rc::new(FunctionInfo {
                    name: function_name,
                    params: vm_params,
                    kwparams: vm_kwparams,
                    entry: 0,
                    return_type,
                    return_julia_type,
                    is_base_extension: func.is_base_extension,
                    is_generated: reflection_meta.is_generated,
                    is_lowering_helper,
                    // Runtime chronology uses zero for non-source rows. Helper
                    // provenance already has a dedicated FunctionInfo field;
                    // never leak the IR sentinel into replacement ordering.
                    definition_order: if is_lowering_helper {
                        0
                    } else {
                        func.span.definition_order
                    },
                    min_world: if is_runtime_eval_function
                        || ((is_current_input_top_level_user_function_info
                            || is_source_ordered_inline_function_info)
                            && is_root_source_function_info)
                    {
                        u64::MAX
                    } else {
                        1
                    },
                    type_params: shared_ctx.expand_type_param_bounds(&func.type_params),
                    param_julia_types,
                    code_start: 0, // Will be set during compilation
                    code_end: 0,   // Will be set during compilation
                    slot_names: Vec::new(),
                    slot_types: Vec::new(),
                    local_slot_count: 0,
                    param_slots: Vec::new(),
                    vararg_param_index,
                    vararg_fixed_count,
                    inlining_meta: reflection_meta.inlining,
                    constprop_meta: reflection_meta.constprop,
                    nospecialize_meta: reflection_meta.nospecialize,
                    propagate_inbounds_meta: reflection_meta.propagate_inbounds,
                    nospecializeinfer_meta: reflection_meta.nospecializeinfer,
                    purity_meta: reflection_meta.purity,
                    direct_return_type_param: direct_return_type_param(func),
                    // 1-based source line of the definition, surfaced as
                    // `Method.line` (Issue #5125).
                    def_line: func.span.start_line as u32,
                    // Issue #10236: suppress the runtime short-name alias
                    // (`VmState::function_name_index`) for a module-body
                    // `let`/`@testset`-root function — see
                    // `module_body_scoped_root_indices`'s doc comment.
                    suppress_short_name_alias: is_module_body_scoped_root,
                    shared_plan: None,
                }));

                // Register function index for Stmt::FunctionDef lookups
                // Use qualified name for nested functions to avoid collisions (Issue #1743)
                let registration_name = if let Some(parent) = func_idx_to_parent.get(&all_funcs_idx)
                {
                    // This is a nested function - use qualified name
                    format!("{}#{}", parent, func.name)
                } else if let Some(module_path) = module_path {
                    format!("{}.{}", module_path, func.name)
                } else {
                    // Top-level or module function - use original name
                    func.name.clone()
                };
                shared_ctx.function_indices.insert(registration_name, idx);
                shared_ctx
                    .function_indices_by_span_start
                    .entry(func.span.start)
                    .or_default()
                    .push(idx);

                // Issue #9189: remember the preload-cache hit (if any) found
                // above, keyed by the now-known `function_infos` index.
                // `compile_functions` consults this to skip codegen for this
                // function entirely; `finalize` splices the cached body in
                // afterward. Recording it here (rather than replacing any
                // FunctionInfo fields right now) keeps "decide whether this
                // is a hit" and "apply a hit" in one place each.
                if let Some(cached) = preload_hit {
                    preload_reused.insert(idx, cached);
                }

                *global_index += 1;
                idx
            };

            // Lazy AoT: Register function if it needs specialization
            // This must be done for ALL functions (including Base when using cache)
            // because cached bytecode may contain CallSpecialized instructions
            // Lazy AoT specialization enabled for:
            // - Base functions: enabled
            // - User functions: enabled
            // - Stdlib modules: enabled (Statistics, etc.)
            // - Core module: DISABLED (intrinsic wrappers like add_int)
            let is_specializable = if let Some(path) = module_path {
                // Module functions: enable for Stdlib, disable for Core
                path != "Core" && !path.starts_with("Core.")
            } else {
                // Non-module functions (Base + User): all enabled
                true
            };
            let specialization_lookup_name =
                if let Some(parent) = func_idx_to_parent.get(&all_funcs_idx) {
                    format!("{}#{}", parent, func.name)
                } else if let Some(module_path) = module_path {
                    format!("{}.{}", module_path, func.name)
                } else {
                    func.name.clone()
                };
            let has_recorded_closure_captures = shared_ctx
                .closure_captures
                .contains_key(&specialization_lookup_name);
            let runtime_specialize = needs_specialization(func) || has_recorded_closure_captures;
            // Issue #5003: register where/value-parametrized methods for reflection-time
            // inference too, but only map to spec_func_mapping (which drives
            // CallSpecialize emission) when runtime specialization is actually needed,
            // so multiple dispatch is preserved for generic methods like promote_rule.
            //
            // Issues #10145 / #10264: also register any *other* module/user-authored
            // method (fully-typed, non-generic params included) for reflection —
            // matching the exact non-Base population
            // `compile::effects::propagation::infer_program_effects` already walks
            // unconditionally. Base/Core functions (`all_funcs_idx < base_function_count`)
            // keep the narrow gate.
            let is_user_defined_for_reflection = all_funcs_idx >= base_function_count;
            let reflection_register =
                needs_reflection_registration(func, is_user_defined_for_reflection);
            if is_specializable && (runtime_specialize || reflection_register) {
                let spec_idx = specializable_functions.len();
                specializable_functions.push(SpecializableFunction {
                    ir: std::sync::Arc::new((*func).clone()),
                    name: func.name.clone(),
                    fallback_index: func_info_idx,
                });
                if runtime_specialize {
                    // Map function global_index to specializable index
                    shared_ctx.spec_func_mapping.insert(func_info_idx, spec_idx);
                }
            }
        }

        // Debug assertion: verify cache alignment after function merging (Issue #2726).
        // When using precompiled cache, all_functions[i] and function_infos[i] must have the same
        // name for all Base functions. A mismatch indicates that exact signature matching in
        // base function filtering has regressed, which would cause Call instructions in cached
        // bytecode to invoke the wrong function.
        #[cfg(debug_assertions)]
        if precompiled_base.is_some() {
            for i in 0..base_function_count
                .min(all_functions.len())
                .min(function_infos.len())
            {
                let all_func_name = &all_functions[i].0.name;
                let info_name = &function_infos[i].name;
                debug_assert_eq!(
                    all_func_name, info_name,
                    "Cache alignment mismatch at index {}: all_functions has '{}' but function_infos has '{}'. \
                     Base function filtering must use exact signature matching (Issue #2726).",
                    i, all_func_name, info_name
                );
            }
        }
        profile::note("compile.build_method_tables.counts", || {
            format!(
                "cached_base_fast={} cached_base_extra={} cached_base_rebuild={} non_cached={} all_functions={} base_function_count={} cached_base_len={} restored_specializable={} restored_runtime_map={}",
                cached_base_fast_count,
                cached_base_extra_reused_count,
                cached_base_rebuild_count,
                non_cached_function_count,
                all_functions.len(),
                base_function_count,
                cached_base_len,
                cached_base_specializations
                    .map(|base| base.specializable_functions.len())
                    .unwrap_or(0),
                cached_base_specializations
                    .map(|base| base.runtime_specialization_map.len())
                    .unwrap_or(0)
            )
        });
        self.cached_base_extra_reused_count = cached_base_extra_reused_count;
    }

    fn register_inner_constructors(
        &mut self,
        inference_engine: &mut abstract_interp::InferenceEngine,
    ) -> CResult<()> {
        let base_function_count = self.base_function_count;
        let precompiled_base = self.precompiled_base;
        // The ORIGINAL cached Base method tables (`method_tables` below is a
        // mutable *clone* of these — see `build_inference_engine`). Used to
        // decide whether a struct's constructors genuinely came from the Base
        // cache, independent of any user methods added in this compilation
        // (Issue #8121).
        let cached_method_tables = self.cached_method_tables;
        let all_structs = &self.all_structs;
        let runtime_inner_constructor_keys = &self.runtime_inner_constructor_keys;
        let function_definition_orders: HashMap<usize, u64> = self
            .all_functions
            .iter()
            .enumerate()
            .filter_map(|(all_functions_idx, (function, _))| {
                if crate::compile::ir_inline::is_markerless_lowered_function(function) {
                    return None;
                }
                self.func_index_map
                    .get(all_functions_idx)
                    .map(|func_info_idx| (*func_info_idx, function.span.definition_order))
            })
            .collect();
        let module_struct_names = &self.module_struct_names;
        let abstract_type_parents = &self.abstract_type_parents;
        let imported_functions = &mut self.imported_functions;
        let method_tables = &mut self.method_tables;
        let shared_ctx = &mut self.shared_ctx;
        let function_infos = &mut self.function_infos;
        let specializable_functions = &mut self.specializable_functions;
        let inner_ctors = &mut self.inner_ctors;
        let global_index = &mut self.global_index;

        // Collect inner constructors from struct definitions (both top-level and module structs)
        // These are registered with the struct name, allowing Point(x, y) to call the inner constructor
        let inner_ctors_timer = profile::start("compile.inner_ctors_collect");
        // Use all_structs to include module structs (e.g., Dates.Date, Dates.DateTime)
        for (struct_def, module_path, prefer_base_origin) in all_structs {
            let qualified_struct_name = module_path
                .as_ref()
                .map(|path| format!("{}.{}", path, struct_def.name))
                .unwrap_or_else(|| struct_def.name.clone());
            let runtime_site_id = runtime_inner_constructor_keys
                .contains(&(
                    qualified_struct_name.clone(),
                    struct_def.span.definition_order,
                    struct_def.span.start,
                ))
                .then_some(struct_def.span.definition_order);

            // Always add struct name to imported_functions (needed for name resolution)
            imported_functions.insert(struct_def.name.clone());

            // When using cache, skip inner constructors that are already in cache
            // (i.e., Base struct inner constructors). User-defined inner constructors
            // need to be registered even when using cache.
            //
            // Issue #8121: the signal must be the ORIGINAL cached Base tables, NOT
            // the working `method_tables`. The working tables are a clone of the
            // cache plus every method registered earlier in `build_method_tables`,
            // so a USER parametric struct `Foo{T}` that also defines an outer
            // constructor `Foo(...)` makes `method_tables["Foo"]` non-empty and was
            // misclassified as a cached Base struct — its inner constructors were
            // then skipped, leaving the bare/braces call to fall back to raw
            // default field construction instead of the user inner/outer ctor.
            // Checking `cached_method_tables` skips genuine Base structs only.
            let skip_this_struct = if precompiled_base.is_some() {
                let is_cached_base_struct = precompiled_base
                    .map(|base| {
                        base.struct_defs.iter().any(|def| {
                            if module_path.is_some() {
                                def.name == qualified_struct_name
                            } else {
                                def.name == struct_def.name
                            }
                        })
                    })
                    .unwrap_or(false);
                is_cached_base_struct
                    && cached_method_tables
                        .and_then(|cached| cached.get(&struct_def.name))
                        .map(|t| !t.methods.is_empty())
                        .unwrap_or(false)
            } else {
                false
            };
            if skip_this_struct {
                continue;
            }

            let constructor_methods: Vec<(
                crate::ir::core::InnerConstructor,
                Option<ConstructorSelfFamily>,
                bool,
            )> = if struct_def.inner_constructors.is_empty() {
                synthetic_default_constructors(
                    struct_def,
                    module_path.as_deref(),
                    module_struct_names,
                )?
                .into_iter()
                .map(|method| {
                    let family = method.constructor_self_family();
                    let is_synthetic_default_outer =
                        method.kind == SyntheticConstructorKind::DefaultOuter;
                    (method.ctor, family, is_synthetic_default_outer)
                })
                .collect()
            } else {
                struct_def
                    .inner_constructors
                    .iter()
                    .cloned()
                    .map(|ctor| {
                        let family = if ctor.is_explicit_parametric {
                            ConstructorSelfFamily::ExplicitParametricInner
                        } else {
                            ConstructorSelfFamily::BareInner
                        };
                        (ctor, Some(family), false)
                    })
                    .collect()
            };
            if constructor_methods.is_empty() {
                continue;
            }
            let has_source_owned_synthetic_default_declaration =
                struct_def.inner_constructors.is_empty()
                    && !struct_def.is_base_origin
                    && !*prefer_base_origin;

            // The declaration owns its concrete-vs-parametric provenance. Bare
            // aliases are callable lookup conveniences and can resolve to an
            // unrelated same-leaf declaration in another owner (Issue #10342).
            // Select the matching registry from StructDef first, then resolve
            // only this declaration's owner-qualified name. In particular, a
            // top-level concrete `Clash` must never make `M.Clash{T}` inherit
            // its type_id and compile `new{T}` as NewStruct (Issue #11147).
            let target = resolve_inner_constructor_target(
                struct_def,
                &qualified_struct_name,
                &shared_ctx.struct_table,
                &shared_ctx.parametric_structs,
            )?;

            for (ctor, constructor_self_family, is_synthetic_default_outer) in &constructor_methods
            {
                // Add struct name to imported_functions immediately when registering inner constructor
                imported_functions.insert(struct_def.name.clone());

                let params: Vec<(String, JuliaType)> = ctor
                    .params
                    .iter()
                    .map(|p| {
                        // Inner constructors are ordinary Julia methods for
                        // dispatch purposes. Normalize their declared types in
                        // exactly the same way as ordinary module methods so a
                        // module-local `::Ring` or `::Partition` matches the
                        // qualified concrete/abstract type at the call site.
                        let ty = p.effective_type();
                        let qualified_ty =
                            qualify_type_for_module(ty, module_path.as_ref(), module_struct_names);
                        let resolved_ty =
                            resolve_abstract_type(qualified_ty, abstract_type_parents);
                        let alias_resolved = resolve_type_alias_in_module_scope(
                            resolved_ty,
                            module_path.as_ref(),
                            &shared_ctx.type_aliases,
                        );
                        (p.name.clone(), alias_resolved)
                    })
                    .collect();

                let vm_params: Vec<(String, ValueType)> = params
                    .iter()
                    .map(|(name, jt)| {
                        (
                            name.clone(),
                            julia_type_to_value_type_scoped(
                                jt,
                                &shared_ctx.struct_table,
                                *prefer_base_origin,
                            ),
                        )
                    })
                    .collect();

                // Inner constructors return the struct type
                // For parametric structs, use Any since actual type is determined at call site
                let (return_type, return_julia_type) = match &target {
                    InnerCtorTarget::Concrete { type_id } => (
                        ValueType::Struct(*type_id),
                        Some(JuliaType::Struct(struct_def.name.clone())),
                    ),
                    InnerCtorTarget::Parametric { .. } => (ValueType::Any, None),
                };

                // Preserve original JuliaTypes for type parameter binding (before params is moved)
                let param_julia_types: Vec<JuliaType> =
                    params.iter().map(|(_, jt)| jt.clone()).collect();

                // Use type params from the inner constructor's where clause
                let ctor_type_params: Vec<TypeParam> = shared_ctx
                    .expand_constructor_type_param_bounds(
                        &ctor.type_params,
                        module_path.as_deref(),
                    );
                let explicit_constructor_type_arguments = shared_ctx
                    .expand_constructor_self_type_arguments(
                        &ctor.explicit_type_arguments,
                        &ctor.type_params,
                        module_path.as_deref(),
                    );

                let method_index = method_tables
                    .get(&struct_def.name)
                    .map_or(0, |table| table.methods.len());
                let mut sig = MethodSig::from_julia_projections(
                    method_index,
                    *global_index,
                    params,
                    return_type.clone(),
                    return_julia_type,
                    false,
                    ctor_type_params.clone(),
                    // Inner constructors don't have varargs.
                    None,
                    None,
                );
                // Workaround: keep Base constructor rows on legacy identity. (Issue #11062)
                // Complete user callable-self metadata currently recurses when
                // applied to Base UnitRange/Array constructor routes.
                if !struct_def.is_base_origin && constructor_self_family.is_some() {
                    sig.explicit_constructor_type_parameter_names =
                        ctor.explicit_type_parameter_names.clone();
                    sig.explicit_constructor_type_arguments =
                        explicit_constructor_type_arguments.clone();
                    sig.explicit_constructor_type_name = Some(qualified_struct_name.clone());
                }

                // Issue #8121: an inner constructor `Foo{T}(args) where {T}` is a
                // DISTINCT method from an outer constructor `Foo(args)` even when
                // their value-parameter signatures coincide — upstream Julia tells
                // them apart by the implicit `Type{Foo{T}}` vs `Type{Foo}` self
                // argument, which sjulia does not model. Both therefore project to
                // the same value-param signature, so `add_method`'s dedup would let
                // this inner ctor REPLACE the already-registered outer (the outer
                // is registered first, in `build_method_tables`). After that, a
                // bare `Foo(args)` call dispatches to the inner — whose `where`
                // type parameters are unbound — instead of the user outer (e.g.
                // `Angle2d{T}(theta::Number) where {T} = new{T}(T(theta))` vs
                // `Angle2d(theta::Number) = Angle2d{...}(theta)` → `UndefVarError:
                // T`). When such a collision is detected, keep BOTH methods (via
                // `add_inner_constructor_method`) so constructor-aware selection
                // can route bare `Foo(args)` to the outer and `Foo{T}(args)` to
                // the inner even when both methods have `where` parameters.
                // Always use the origin-aware insertion path. Predicting
                // whether an outer method collides here is unsound unless it
                // exactly mirrors MethodTable's canonical dedup rules; a
                // missed collision lets ordinary `add_method` replace the
                // outer with the inner (or vice versa). The insertion method
                // preserves outer rows and deduplicates only already-recorded
                // inner constructors (Issue #10959).
                //
                // The constructor's self family (bare vs explicit-parametric)
                // is recorded directly on the method table's serialized
                // origin carrier rather than a transient compile-session
                // HashSet, so it survives Base-cache serialization and every
                // table clone/filter path (Issue #10962, #10974).
                let mut table_names = vec![struct_def.name.clone()];
                if qualified_struct_name != struct_def.name {
                    table_names.push(qualified_struct_name.clone());
                }
                let mut registered_any = false;
                for table_name in table_names {
                    // A user struct constructor shares the bare constructor-family
                    // table with a same-leaf Base type. Preserve the Base-owned
                    // rows before an implicit/explicit inner constructor can
                    // replace an equal value signature; qualified `Base.Set(...)`
                    // must never resolve to a module-local `Set` (Issue #11367).
                    if !struct_def.is_base_origin
                        && !*prefer_base_origin
                        && !table_name.contains('.')
                        && !method_tables.contains_key(&format!("Base.{table_name}"))
                    {
                        let base_methods = method_tables.get(&table_name).map(|table| {
                            table
                                .methods
                                .iter()
                                .filter(|method| {
                                    method.is_base_program_method(base_function_count)
                                        || method.is_base_extension
                                })
                                .cloned()
                                .collect::<Vec<_>>()
                        });
                        if let Some(base_methods) = base_methods.filter(|rows| !rows.is_empty()) {
                            let qualified_base = format!("Base.{table_name}");
                            let mut snapshot = MethodTable::new(qualified_base.clone());
                            snapshot.set_base_function_count(base_function_count);
                            for method in base_methods {
                                snapshot.add_method(method.clone());
                                inference_engine.add_initial_method(qualified_base.clone(), method);
                            }
                            method_tables.insert(qualified_base, snapshot);
                        }
                    }
                    let table = method_tables
                        .entry(table_name.clone())
                        .or_insert_with(|| MethodTable::new(table_name.clone()));
                    // Declaration provenance must be recorded before the
                    // last-definition-wins skip below: an all-Any synthetic
                    // outer can be completely replaced by a later source row,
                    // yet calls for this owner must still use normal dispatch.
                    if has_source_owned_synthetic_default_declaration {
                        table
                            .mark_synthetic_default_constructor_declaration(&qualified_struct_name);
                    }
                    // Struct declarations and ordinary methods are collected
                    // in separate passes, so insertion order alone is not
                    // Julia evaluation order. Lowering assigns a monotonic
                    // definition ordinal shared across recursively included
                    // files; raw byte offsets are file-local and cannot make
                    // this comparison. A later ordinary bare constructor wins;
                    // an earlier one is replaced by this inner. Explicit-
                    // parametric inners have a distinct callable self and never
                    // enter this comparison (Issue #11028).
                    let shares_bare_callable_self = constructor_self_family
                        .is_none_or(|family| family == ConstructorSelfFamily::BareInner);
                    let later_ordinary_constructor_wins = shares_bare_callable_self
                        && table
                            .ordinary_method_with_same_signature(&sig)
                            .and_then(|global_index| {
                                function_definition_orders.get(&global_index).copied()
                            })
                            .is_some_and(|order| order > struct_def.span.definition_order);
                    if !later_ordinary_constructor_wins {
                        if let Some(family) = constructor_self_family {
                            table.add_inner_constructor_method(sig.clone(), *family);
                        } else if *is_synthetic_default_outer
                            && has_source_owned_synthetic_default_declaration
                        {
                            table.add_synthetic_default_outer_method(
                                sig.clone(),
                                &qualified_struct_name,
                            );
                        } else {
                            table.add_method(sig.clone());
                        }
                        inference_engine.add_initial_method(table_name, sig.clone());
                        registered_any = true;
                    }
                }
                if !registered_any {
                    continue;
                }

                // Record the index where this inner constructor will be stored
                let func_info_idx = function_infos.len();
                let runtime_constructor_name = if constructor_self_family.is_none()
                    || (!struct_def.is_base_origin
                        && matches!(
                            constructor_self_family,
                            Some(ConstructorSelfFamily::BareInner)
                        )) {
                    qualified_struct_name.clone()
                } else if !struct_def.is_base_origin
                    && ctor.is_explicit_parametric
                    && !ctor.explicit_type_arguments.is_empty()
                {
                    TypeExpr::format_parameterized(
                        &qualified_struct_name,
                        &explicit_constructor_type_arguments,
                    )
                } else {
                    struct_def.name.clone()
                };

                function_infos.push(std::rc::Rc::new(FunctionInfo {
                    // Runtime DataType calls recover the constructor self from
                    // this name. Preserve `Foo{T}` for braced user inner
                    // constructors so ApplyTypeDynamic includes them for an
                    // explicit call while a bare `Foo(...)` call does not.
                    name: runtime_constructor_name,
                    params: vm_params,
                    kwparams: vec![],
                    entry: 0,
                    return_type,
                    return_julia_type: None,
                    is_base_extension: false,
                    is_generated: false,
                    is_lowering_helper: false,
                    definition_order: ctor.span.definition_order,
                    min_world: runtime_site_id.map_or(1, |_| u64::MAX),
                    type_params: ctor_type_params,
                    param_julia_types,
                    code_start: 0, // Will be set during compilation
                    code_end: 0,   // Will be set during compilation
                    slot_names: Vec::new(),
                    slot_types: Vec::new(),
                    local_slot_count: 0,
                    param_slots: Vec::new(),
                    vararg_param_index: None, // Inner constructors don't have varargs
                    vararg_fixed_count: None,
                    inlining_meta: 0,
                    constprop_meta: 0,
                    nospecialize_meta: 0,
                    propagate_inbounds_meta: false,
                    nospecializeinfer_meta: false,
                    purity_meta: 0,
                    direct_return_type_param: None,
                    // Inner constructors report the struct definition's source line
                    // (Issue #5125).
                    def_line: struct_def.span.start_line as u32,
                    suppress_short_name_alias: false,
                    shared_plan: None,
                }));

                inner_ctors.push(InnerCtorInfo {
                    target: target.clone(),
                    is_base_origin: struct_def.is_base_origin,
                    ctor: ctor.clone(),
                    func_info_idx,
                    module_path: module_path.clone(),
                });
                if let Some(site_id) = runtime_site_id {
                    shared_ctx
                        .runtime_nominal_constructor_indices
                        .entry(site_id)
                        .or_default()
                        .push(func_info_idx);
                }

                // Issue #4848: retain the inner constructor IR in
                // `specializable_functions` so reflection-time inference can analyze
                // the constructor body (e.g. `new(x, "x")`) and recover
                // PartialStruct-style field facts across the constructor return
                // boundary. Only non-parametric immutable constructors are
                // registered: parametric constructors resolve their concrete type at
                // the call site (return Any here), and mutable structs do not
                // preserve field-value facts. This does NOT add the constructor to
                // `spec_func_mapping`, so dispatch/codegen is unaffected.
                if matches!(&target, InnerCtorTarget::Concrete { .. }) && !struct_def.is_mutable {
                    let ctor_ir = crate::ir::core::Function {
                        name: struct_def.name.clone(),
                        params: ctor.params.clone(),
                        kwparams: ctor.kwparams.clone(),
                        type_params: shared_ctx.expand_type_param_bounds(&ctor.type_params),
                        return_type: None,
                        body: ctor.body.clone(),
                        is_base_extension: false,
                        is_runtime_eval: false,
                        span: ctor.span,
                        new_struct_name: None,
                    };
                    specializable_functions.push(SpecializableFunction {
                        ir: std::sync::Arc::new(ctor_ir),
                        name: struct_def.name.clone(),
                        fallback_index: func_info_idx,
                    });
                }

                *global_index += 1;
            }
        }
        // Also add struct names to imported_functions so they can be called
        // Use all_structs to include module structs
        for (struct_def, _module_path, _) in all_structs {
            if !struct_def.inner_constructors.is_empty() {
                imported_functions.insert(struct_def.name.clone());
            }
        }
        profile::finish(inner_ctors_timer);
        Ok(())
    }

    fn project_method_table_hierarchy(&mut self) {
        let base_function_count = self.base_function_count;
        let shared_ctx = &self.shared_ctx;
        let method_tables = &mut self.method_tables;

        // Populate struct_parents on all method tables for abstract dispatch tie-breaking (Issue #3144).
        // Build a map from concrete struct name to its declared parent abstract type.
        // This enables `dispatch()` to correctly prefer f(::MotorVehicle) over f(::NonMotorVehicle)
        // when the argument is Car where `struct Car <: MotorVehicle`.
        {
            let hierarchy_projection_timer =
                profile::start("compile.method_table_hierarchy_projection");
            let struct_hierarchy = build_struct_hierarchy_from_context(shared_ctx);
            let concrete_struct_names: Vec<String> = shared_ctx
                .struct_defs
                .iter()
                .map(|def| def.name.clone())
                .collect();
            let parametric_struct_names: Vec<String> =
                shared_ctx.parametric_structs.keys().cloned().collect();
            let abstract_type_names: Vec<String> = shared_ctx
                .abstract_types
                .iter()
                .map(|at| at.name.clone())
                .collect();

            // Issue #5646: parametric user structs (`struct Circle{T} <: Shape`) are
            // NOT in `struct_defs` — they instantiate lazily and live in
            // `parametric_structs` (Issue #5052). Without their declared parent here,
            // a `where {T<:Shape}` method failed to match a parametric argument
            // (`Circle{Float64}`): the struct-parent fallback fell into the
            // "conservatively accept unknown struct" branch, which is then either
            // rejected by the missing match arm or would wrongly accept an unrelated
            // struct. Seed every parametric struct's (base name -> declared parent
            // base name), including parentless ones (mapped to `None`), so the chain
            // walk in `struct_is_subtype_of_abstract` accepts `Circle <: Shape` and
            // rejects an unrelated `Box{T}`.

            // Issue #5056: user *abstract* type → declared parent links, kept in a
            // separate map so `struct_parents` stays struct-only (Issue #3144
            // tie-breaking). The dispatch subtype walk consults this to follow a
            // multi-level chain through intermediate user abstracts before reaching
            // a built-in abstract (`struct Tiny <: MyInt`, `abstract type MyInt <:
            // MyNum`, `abstract type MyNum <: Number` ⇒ `Tiny` dispatches `::Number`).

            // Issue #6348: the projection only depends on the program-wide
            // struct/abstract definitions, so build it ONCE and share the same
            // `Arc` across all (1100+) method tables instead of rebuilding and
            // cloning the full hierarchy per table (~37 ms per warm run).
            let shared_projection =
                std::sync::Arc::new(method_table::MethodTableProjection::build(
                    &struct_hierarchy,
                    &concrete_struct_names,
                    &parametric_struct_names,
                    &abstract_type_names,
                ));
            for table in method_tables.values_mut() {
                table.set_base_function_count(base_function_count);
                table.set_shared_projection(std::sync::Arc::clone(&shared_projection));
            }

            // Issue #5920: MethodTable keeps the shared hierarchy explicitly; do
            // not seed the inference thread-local registry from compile.
            profile::finish(hierarchy_projection_timer);
        }
    }

    fn analyze_module_lambda_captures(&mut self) {
        let program = self.program;
        let base_function_count = self.base_function_count;
        let opt_user_functions = self.opt_user_functions;
        let opt_main = self.opt_main;
        let inline_functions = self.inline_functions;
        let shared_ctx = &mut self.shared_ctx;

        // Pre-analyze closure captures for lambda functions defined at module level (Issue #2358)
        //
        // Lambda functions (e.g., `f = () -> x + 1`) in @testset or other module-level blocks
        // are lifted to top-level functions named __lambda_N. They need to capture variables
        // from the outer scope. This must be done BEFORE the function compilation loop.
        //
        // First, collect the module-level local binding *names* to know what
        // variables are available. Capture analysis only consumes the name set,
        // so the legacy typed pre-scan (which also computed a ValueType per
        // binding and mixed-type tracking, all discarded here) is replaced by
        // the name-only walker (Issue #5922).
        {
            let lambda_captures_timer = profile::start("compile.module_lambda_captures");
            let mut module_scope_vars: HashSet<String> = HashSet::new();
            collect_local_binding_names_for_capture(&opt_main.stmts, &mut module_scope_vars);
            collect_testset_local_binding_names_for_capture(
                &opt_main.stmts,
                &mut module_scope_vars,
            );

            // Index all marker-less lifted helper functions by provenance, not
            // by their generated spelling. Do-block / arrow lambdas are lifted
            // FLAT to the top level (Issue #7600): a do-block
            // nested inside another do-block becomes two sibling top-level
            // functions, with the outer one referencing the inner one by name.
            // The nesting relationship — needed so the inner lambda can capture
            // the outer lambda's params / locals — is recovered from those
            // references below.
            // `__gen_body_N` functions (generator bodies lifted by lowering,
            // Issue #9103) are module-level lambdas in the same sense and
            // need the identical capture analysis: a top-level `let` /
            // `@testset` generator body referencing a block local must
            // capture it, not read a (nonexistent) global.
            let lambda_funcs: HashMap<&str, &Function> = program
                .functions
                .iter()
                .take(base_function_count)
                .map(|f| f.as_ref())
                .chain(opt_user_functions.iter())
                .chain(
                    // Top-level inline FunctionDefs (parent None): the lifted
                    // `__gen_body_N` definitions ride inside main statements
                    // (a LetBlock), not the program function list, so they
                    // only appear here.
                    inline_functions
                        .iter()
                        .filter(|(_, parent)| parent.is_none())
                        .map(|(func, _)| func),
                )
                .filter(|f| crate::compile::ir_inline::is_markerless_lowered_function(f))
                .map(|f| (f.name.as_str(), f))
                .collect();

            // The local scope each lambda contributes to its nested lambdas:
            // its own parameters plus the names it binds in its body.
            let lambda_local_scope: HashMap<&str, HashSet<String>> = lambda_funcs
                .iter()
                .map(|(&name, &func)| {
                    let mut scope: HashSet<String> =
                        func.params.iter().map(|p| p.name.clone()).collect();
                    collect_local_binding_names_for_capture(&func.body.stmts, &mut scope);
                    (name, scope)
                })
                .collect();

            // parent[child] = the lambda whose body references `child`. A
            // nested do-block is referenced from exactly one enclosing lambda.
            let mut parent_of: HashMap<&str, &str> = HashMap::new();
            for (&name, &func) in &lambda_funcs {
                for referenced in collect_referenced_names(func) {
                    if let Some((&child_name, _)) = lambda_funcs.get_key_value(referenced.as_str())
                    {
                        if child_name != name {
                            parent_of.entry(child_name).or_insert(name);
                        }
                    }
                }
            }

            // Depth of each lambda in the parent forest (root = 0), and the
            // direct free variables it references from an outer scope. The outer
            // scope is the module bindings plus the local scope of every
            // enclosing lambda, so a nested do-block can reference the outer
            // do-block's params / locals.
            let depth_of = |start: &str| -> usize {
                let mut depth = 0usize;
                let mut cur = parent_of.get(start).copied();
                let mut guard = 0usize;
                while let Some(anc) = cur {
                    depth += 1;
                    cur = parent_of.get(anc).copied();
                    guard += 1;
                    if guard > lambda_funcs.len() {
                        break; // defensive bound against a malformed cycle
                    }
                }
                depth
            };

            let mut direct_free: HashMap<&str, HashSet<String>> = HashMap::new();
            for (&name, &func) in &lambda_funcs {
                let mut outer_scope_vars = module_scope_vars.clone();
                let mut ancestor = parent_of.get(name).copied();
                let mut guard = 0usize;
                while let Some(anc) = ancestor {
                    if let Some(scope) = lambda_local_scope.get(anc) {
                        outer_scope_vars.extend(scope.iter().cloned());
                    }
                    ancestor = parent_of.get(anc).copied();
                    guard += 1;
                    if guard > lambda_funcs.len() {
                        break;
                    }
                }
                direct_free.insert(name, analyze_free_variables(func, &outer_scope_vars));
            }

            // Propagate captures bottom-up (children before parents) so that an
            // intermediate lambda also captures any variable a *descendant*
            // lambda needs from a scope above it — the "capture to pass it down"
            // chain that mirrors the named-nested-function deep-nesting analysis
            // (Issue #1744), but for flat lifted do-block / arrow lambdas
            // (Issue #7600). A name bound in the lambda's own scope is available
            // directly when it builds the child closure, so it is dropped from
            // the lambda's own capture set.
            let mut names_by_depth_desc: Vec<&str> = lambda_funcs.keys().copied().collect();
            names_by_depth_desc.sort_by_key(|n| std::cmp::Reverse(depth_of(n)));

            let mut captures: HashMap<&str, HashSet<String>> = HashMap::new();
            for &name in &names_by_depth_desc {
                let mut caps = direct_free.get(name).cloned().unwrap_or_default();
                // Pull up any still-unsatisfied captures of direct children.
                for (&child, &parent) in &parent_of {
                    if parent == name {
                        if let Some(child_caps) = captures.get(child) {
                            caps.extend(child_caps.iter().cloned());
                        }
                    }
                }
                // Drop names bound in this lambda's own scope: they resolve from
                // this frame, not from a capture.
                if let Some(scope) = lambda_local_scope.get(name) {
                    caps.retain(|n| !scope.contains(n));
                }
                captures.insert(name, caps);
            }

            for (&name, caps) in &captures {
                if !caps.is_empty() {
                    shared_ctx
                        .closure_captures
                        .insert(name.to_string(), caps.clone());
                }
            }
            profile::finish(lambda_captures_timer);
        }

        // Named functions defined inside a module-level `let` scope capture that
        // scope's locals — the Base bootstrap pattern
        // (`let counter = 0; global function inc() counter += 1 end; end`,
        // Issue #11015). Their bodies are compiled by `compile_functions`, which
        // runs BEFORE `compile_main` ever reaches the `let`, so the capture set
        // must be known here; otherwise the body compiles against a (nonexistent)
        // global and fails with `UndefVarError` at call time.
        //
        // Only real-binding `let` scopes qualify: a bare top-level assignment is a
        // module global, which a function must keep reading dynamically.
        let mut let_captures = LetScopeCaptures::default();
        collect_let_scope_function_captures(
            &self.opt_main.stmts,
            &HashSet::new(),
            &mut let_captures,
        );
        for (name, caps) in let_captures.captures {
            if !caps.is_empty() {
                self.shared_ctx.closure_captures.insert(name, caps);
            }
        }
        self.shared_ctx.let_scope_global_closures = let_captures.global_closures;
    }

    fn compile_functions(&mut self) -> CResult<()> {
        let program = self.program;
        let mut bare_module_paths = HashSet::new();
        for module in &self.all_modules {
            collect_bare_module_paths(module, "", &mut bare_module_paths);
        }
        let toplevel_module_bindings = self.toplevel_module_bindings();
        self.reused_base = if self.precompiled_base.is_some() {
            vec![true; self.function_infos.len()]
        } else {
            vec![false; self.function_infos.len()]
        };
        let precompiled_base = self.precompiled_base;
        let base_function_count = self.base_function_count;
        let cached_base_len = self.cached_base_len;
        let inline_start_idx = self.inline_start_idx;
        let first_user_function_idx = self.first_user_function_idx;
        let all_functions = &self.all_functions;
        let func_idx_to_parent = &self.func_idx_to_parent;
        let func_index_map = &self.func_index_map;
        let base_function_names = &self.base_function_names;
        let shadowed_user_globals = &self.shadowed_user_globals;
        let imported_functions = &self.imported_functions;
        let module_functions = &self.module_functions;
        let module_imports_map = &self.module_imports_map;
        let module_usings_map = &self.module_usings_map;
        let method_tables = &self.method_tables;
        let module_exports = &self.module_exports;
        let usings_set = &self.usings_set;
        let abstract_type_names = &self.abstract_type_names;
        let module_constants = &self.module_constants;
        let user_function_names = &self.user_function_names;
        let function_infos = &mut self.function_infos;
        let reused_base = &mut self.reused_base;
        let code = &mut self.code;
        let source_map = &mut self.source_map;
        let shared_ctx = &mut self.shared_ctx;
        // Issue #9189: functions `build_method_tables` already matched against
        // the preloaded-package cache. Read-only here (the loop below only
        // needs to know "skip or not"); `finalize` takes ownership of this
        // map to actually splice the cached bodies in.
        let preload_reused = &self.preload_reused;

        // Compile each function.
        //
        // Keep cached Base bytecode out of the mutable suffix while compiling user
        // functions/main. The final `CompiledProgram.code` is still a single Vec,
        // but deferring the Base prefix copy lets slotization and peephole passes run
        // on the user/main suffix only instead of repeatedly copying the cached Base
        // prefix through protected ranges (Issue #6348).
        let emit_functions_timer = profile::start("compile.emit_functions");
        // SSA pipeline gate (Issue #8552): read the env var once per compile.
        // With the gate off this check is the only overhead of the SSA path.
        let ssa_pipeline = super::ssa_ir::ssa_pipeline_enabled();
        // Body-derived effect summaries (#8441) for the SSA passes (flip
        // criterion 4, `docs/vm/SSA_IR.md`): computed lazily, once per gated
        // program compile, when the first eligible user function reaches the
        // SSA path. Gate-off compiles never pay for this. `resolver` (Issue
        // #9495) additionally carries the per-method summaries the DCE/CSE
        // gates consult at statically-resolved call sites; `None` when no
        // fully-visible multi-method generic exists (the common case).
        struct SsaEffects {
            by_name: std::collections::HashMap<
                super::effects::propagation::FuncId,
                super::effects::Effects,
            >,
            resolver: Option<super::effects::static_dispatch::StaticDispatchResolver>,
        }
        let mut ssa_effects: Option<SsaEffects> = None;
        let mut assigned_user_globals = HashSet::new();
        if let Some(idx) = self
            .opt_main
            .stmts
            .iter()
            .position(is_base_user_main_boundary)
        {
            collect_assigned_binding_names(
                &self.opt_main.stmts[idx + 1..],
                &mut assigned_user_globals,
            );
        }
        // Assigned user-main bindings are real globals visible to user functions,
        // even when a Base/prelude method table has the same short name (e.g.
        // `const lt = (<:)` vs Base ordering's `lt`; Issue #8911).
        let shadowed_global_types = shadowed_user_globals
            .iter()
            .map(|name| {
                let saved = shared_ctx.global_types.remove(name);
                let restore = if user_function_names.contains(name)
                    && !assigned_user_globals.contains(name)
                {
                    None
                } else {
                    saved
                };
                (name.clone(), restore)
            })
            .collect::<Vec<_>>();
        for (idx, (func, module_path)) in all_functions.iter().enumerate() {
            // Fast-path for cached Base functions: skip ALL per-iteration work.
            //
            // All cached Base functions occupy indices 0..base_function_count in
            // all_functions, so they are processed before any user function. At
            // that point the global_types manipulations below (remove for non-user
            // scope) are no-ops — the shadowed names were already removed from
            // global_types before this loop started. Jumping past them for ~4969
            // cached functions avoids O(base_count × shadowed_count) HashMap
            // operations on every cold start (Issue #9104).
            //
            // Safety: `func_index_map` is populated for all indices in
            // build_method_tables before compile_functions is called, so
            // func_index_map[idx] is always valid here.
            if precompiled_base.is_some() {
                let func_info_idx = func_index_map[idx];
                if func_info_idx < cached_base_len {
                    let fi = &function_infos[func_info_idx];
                    if fi.code_start != fi.code_end {
                        // Valid cached bytecode — nothing to emit.
                        continue;
                    }
                }
            }

            let is_user_function_scope = if idx >= inline_start_idx {
                func_idx_to_parent
                    .get(&idx)
                    .map(|parent| user_function_names.contains(parent))
                    .unwrap_or(true)
            } else {
                idx >= first_user_function_idx
            };
            if is_user_function_scope {
                for (name, ty) in &shadowed_global_types {
                    if let Some(ty) = ty {
                        shared_ctx.global_types.insert(name.clone(), ty.clone());
                    }
                }
            } else {
                for name in shadowed_user_globals {
                    shared_ctx.global_types.remove(name);
                }
            }
            let hides_user_globals = idx < self.base_function_count
                || func_idx_to_parent
                    .get(&idx)
                    .is_some_and(|parent| base_function_names.contains(parent));
            // Map all_functions index to function_infos index
            let func_info_idx = func_index_map[idx];

            // Issue #9189: this function's body was already compiled when the
            // preloaded-package cache was generated — skip semantic
            // compilation (inference/codegen) entirely.
            //
            // Only the SLOT LAYOUT is copied from the cache, not the whole
            // `FunctionInfo`: `params`/`return_type`/`kwparams[].ty` are
            // `ValueType`, and `ValueType::Struct(type_id)` embeds a
            // `next_type_id` running counter assigned while THIS compile's
            // struct table is built (`context.rs`) — a struct's id in the
            // isolated single-package cache-generation compile need not
            // match its id in a real, multi-module compile, so reusing those
            // fields wholesale would silently corrupt struct-typed dispatch
            // (a first fix attempt here that did exactly that produced the
            // `MethodError: Any is ambiguous` failures across the fixture
            // suite — reverted). `build_method_tables`'s freshly-computed
            // `params`/`return_type`/`kwparams` for THIS run's live
            // struct_table are kept as-is.
            //
            // The SLOT layout, in contrast, is purely a function-local
            // analysis over that one function's own params/kwparams/body —
            // unaffected by which other modules are compiled alongside it —
            // so it's safe (and necessary) to take from the cache: the
            // cached body's bytecode was slotized against exactly these slot
            // assignments. `kwparams[].slot` specifically is only assigned by
            // `finalize`'s slotize loop, which this hit skips (`reused_base
            // = true`, since the cached body is already slotized/peepholed
            // final-form and re-slotizing it would corrupt it) — leaving it
            // at build_method_tables's placeholder (`0`) silently pointed
            // kwarg-derived locals (e.g. `aspect_ratio`) at the wrong slot;
            // that was the actual, narrower bug behind the same failures.
            //
            // Unlike the true cached-Base prefix, this function's absolute
            // position in the code buffer isn't known yet (it varies per run
            // — only Base is always at position 0), so its body is NOT
            // appended to `code` here; `finalize` splices it in after both
            // whole-buffer peephole passes have run (and sets `entry`/
            // `code_start`/`code_end` there), the same relocate-and-append
            // primitive `compile_module_recursive`/`compile_main` use for
            // their own chunks.
            if let Some(cached) = preload_reused.get(&func_info_idx) {
                let fi = std::rc::Rc::make_mut(&mut function_infos[func_info_idx]);
                fi.slot_names = cached.function_info.slot_names.clone();
                fi.slot_types = cached.function_info.slot_types.clone();
                fi.param_slots = cached.function_info.param_slots.clone();
                fi.local_slot_count = cached.function_info.local_slot_count;
                for (kw, cached_kw) in fi
                    .kwparams
                    .iter_mut()
                    .zip(cached.function_info.kwparams.iter())
                {
                    kw.slot = cached_kw.slot;
                }
                reused_base[func_info_idx] = true;
                continue;
            }

            let entry = code.len();
            // User function being compiled this run: unshared Rc, in-place mutation.
            std::rc::Rc::make_mut(&mut function_infos[func_info_idx]).entry = entry;
            reused_base[func_info_idx] = false; // This is a user function, not reused from cache

            let mut function_imports = imported_functions.clone();
            function_imports.insert(func.name.clone());
            if let Some(module_path) = module_path {
                if let Some(module_funcs) = module_functions.get(module_path) {
                    function_imports.extend(module_funcs.iter().cloned());
                }
                if let Some(module_imports) = module_imports_map.get(module_path) {
                    function_imports.extend(module_imports.iter().cloned());
                }
            }
            // Issue #10220/#10294: a top-level (`module_path == None`)
            // Base/prelude function must never fall back to
            // `program.usings` — that flat, whole-program list also
            // contains the USER SCRIPT's own top-level `using` statements
            // (both have `module_path == None`, so they are otherwise
            // indistinguishable here). Defense in depth: the decisive fix
            // is `visible_using_modules_for_name`'s own
            // `in_base_function_scope` guard (`self.usings`'s
            // always-global fallback would otherwise still leak through
            // even with `resolved_usings` hardened alone), but keeping
            // `resolved_usings` itself free of cross-scope `using`s for a
            // Base-origin top-level function closes the same gap for any
            // other consumer of `CoreCompiler::resolved_usings`.
            let function_scope_usings: &[UsingImport] = if hides_user_globals {
                &[]
            } else {
                module_path
                    .as_ref()
                    .and_then(|path| module_usings_map.get(path))
                    .map(Vec::as_slice)
                    .unwrap_or(program.usings.as_slice())
            };
            let resolved_usings = resolve_scope_using_imports(
                function_scope_usings,
                module_path.as_deref().unwrap_or(""),
                module_functions,
            );
            // Check if this function is a closure with captured variables
            // Clone the captures before creating the compiler (to avoid borrow conflicts)
            //
            // For nested functions, closure_captures uses qualified names like "parent#nested"
            // We use func_idx_to_parent to find the exact parent for this function index,
            // which allows disambiguating between multiple nested functions with the same name
            // from different parents (Issue #1743).
            let closure_captures = if let Some(parent) = func_idx_to_parent.get(&idx) {
                // This is a nested function - look up by qualified name
                let qualified_name = format!("{}#{}", parent, func.name);
                shared_ctx
                    .closure_captures
                    .get(&qualified_name)
                    .cloned()
                    .unwrap_or_default()
            } else {
                // Top-level or module function - look up by simple name
                shared_ctx
                    .closure_captures
                    .get(&func.name)
                    .cloned()
                    .unwrap_or_default()
            };
            let normalized_type_params = shared_ctx.expand_type_param_bounds(&func.type_params);

            let mut compiler = CoreCompiler::new_for_function(
                method_tables,
                module_functions,
                module_exports,
                &function_imports,
                usings_set,
                resolved_usings,
                shared_ctx,
                abstract_type_names,
                module_constants,
            );
            compiler.toplevel_module_bindings = toplevel_module_bindings.clone();
            compiler.current_module_is_bare = module_path
                .as_ref()
                .is_some_and(|path| bare_module_paths.contains(path));
            if hides_user_globals {
                compiler.hidden_user_globals = shadowed_user_globals.clone();
            }

            // Set captured_vars so that load_local emits LoadCaptured for those variables
            compiler.captured_vars = closure_captures;

            // Set the current function name for nested function disambiguation
            // For nested functions, use the qualified name (parent#nested) so that
            // deeper nesting levels can build the full qualified path (Issue #1744).
            // A top-level MODULE function (no func_idx_to_parent entry) must use
            // its own module-qualified name ("Module.path.func"), matching the
            // identity `function_infos`/`function_indices`/method-table
            // registration below already use (`function_name`/`registration_name`)
            // — otherwise two modules' same-named top-level function (e.g. both
            // declaring `function outer() ... end`) would both set
            // `current_function_name = "outer"` here, so a nested helper inside
            // EITHER module's `outer` would build the SAME bare qualified name
            // ("outer#helper"), colliding in `function_indices`/`closure_captures`
            // and in the nested method-table entry those registration sites key
            // by — one module's call to its own helper could silently execute the
            // OTHER module's helper body (Issue #10214).
            let current_func_name = if let Some(parent) = func_idx_to_parent.get(&idx) {
                format!("{}#{}", parent, func.name)
            } else if let Some(module_path) = module_path {
                format!("{}.{}", module_path, func.name)
            } else {
                func.name.clone()
            };
            compiler.current_function_name = Some(current_func_name);

            // Set module path for resolving unqualified struct names inside module functions
            compiler.current_module_path = module_path.clone();
            // Names imported into this module via `using`/`import` keep cross-module
            // dispatch pooling and must NOT be redirected to the module-owned table
            // (Issue #7575).
            compiler.current_module_imports = module_path
                .as_ref()
                .and_then(|path| module_imports_map.get(path))
                .cloned()
                .unwrap_or_default();
            compiler.in_base_function_scope = idx < base_function_count
                || func_idx_to_parent
                    .get(&idx)
                    .is_some_and(|parent| base_function_names.contains(parent));

            // A `global` helper declared inside a struct body is an ordinary
            // global method, but its body keeps the struct body's privileged
            // access to `new` / `new{T}` — upstream's `unsafe_rational` shape
            // (Issue #11005). Give it the same `new`-resolution context the
            // struct's inner constructors get.
            if let Some(struct_name) = &func.new_struct_name {
                if let Some(info) = compiler.shared_ctx.struct_table.get(struct_name) {
                    compiler.current_struct_type_id = Some(info.type_id);
                } else if compiler
                    .shared_ctx
                    .parametric_structs
                    .contains_key(struct_name)
                {
                    compiler.current_struct_type_id = Some(0);
                    compiler.current_parametric_struct_name = Some(struct_name.clone());
                }
            }

            // Set type parameters from where clause for type binding support
            compiler.current_type_params = normalized_type_params.clone();
            compiler.current_type_param_index = normalized_type_params
                .iter()
                .enumerate()
                .map(|(i, tp)| (tp.name.clone(), i))
                .collect();

            // Collect type parameter names from the function's where clause
            let func_type_param_names: HashSet<&str> = normalized_type_params
                .iter()
                .map(|tp| tp.name.as_str())
                .collect();

            // Detect Val{N} patterns and mark N as a value parameter
            // For parameters like ::Val{N} where N, N should be treated as I64, not DataType
            for param in &func.params {
                if let JuliaType::Struct(type_name) = param.effective_type() {
                    if type_name.starts_with("Val{") && type_name.ends_with("}") {
                        // Extract the type argument (e.g., "N" from "Val{N}")
                        let type_arg = &type_name[4..type_name.len() - 1];
                        // If this type arg is a where clause type parameter, it's a value parameter
                        if func_type_param_names.contains(type_arg) {
                            compiler.val_type_params.insert(type_arg.to_string());
                        }
                    } else if type_name.starts_with("NTuple{") && type_name.ends_with("}") {
                        // Collect every length value parameter, including those of
                        // nested NTuple element types such as `NTuple{N,NTuple{M,T}}`
                        // where both N and M are value parameters (Issue #4842).
                        collect_ntuple_value_params(
                            &type_name,
                            &func_type_param_names,
                            &mut compiler.val_type_params,
                        );
                    } else {
                        collect_array_rank_value_params(
                            &type_name,
                            &func_type_param_names,
                            &mut compiler.val_type_params,
                        );
                    }
                }
            }

            // Set up parameter types in locals
            for param in &func.params {
                let param_ty = param.effective_type();
                // Ensure parametric struct instantiations exist (e.g., Complex{Float64})
                if let JuliaType::Struct(name) = &param_ty {
                    if name.contains('{') && !compiler.shared_ctx.struct_table.contains_key(name) {
                        // Parse type arguments and create instantiation
                        if let Some(brace_idx) = name.find('{') {
                            let base_name = &name[..brace_idx];
                            let type_args_str = &name[brace_idx + 1..name.len() - 1];

                            // Check if any type arg is a where clause type parameter
                            let Some(type_args) = parse_type_args_recursive(type_args_str) else {
                                continue;
                            };
                            let has_type_param = type_args.iter().any(|arg| {
                                type_expr_contains_type_param(arg, &func_type_param_names)
                            });

                            // Skip instantiation if any type arg is a where clause type parameter
                            if !has_type_param {
                                let _ = compiler
                                    .shared_ctx
                                    .resolve_instantiation_with_type_expr(base_name, &type_args);
                            }
                        }
                    }
                }
                // Varargs parameters are bound as a Tuple collector at runtime,
                // even when each accepted argument has a typed annotation such as
                // `xs::Vector{Int64}...` (Issue #3914).
                let vt = if param.is_varargs {
                    ValueType::Tuple
                } else if matches!(param_ty, JuliaType::Dict) {
                    // Bare `::Dict` is the `Dict{K,V}` UnionAll family after
                    // Value::Dict carrier removal. Keep parameter storage
                    // dynamic so method bodies use pure-Julia Dict dispatch
                    // instead of legacy Dict slot/builtin paths. (Issue #7632)
                    ValueType::Any
                } else {
                    compiler.julia_type_to_value_type_with_ctx(&param_ty)
                };
                compiler.locals.insert(param.name.clone(), vt.clone());
                compiler.initialized_locals.insert(param.name.clone());
                // Track parameters with JuliaTypes that ValueType cannot represent
                // precisely, so infer_julia_type can recover the dispatch type.
                // This includes narrow integers (e.g., Int32 instead of ValueType::I64)
                // and parametric arrays (e.g., Vector{Int32} instead of ValueType::Array).
                // This is needed for correct compile-time dispatch of calls like
                // gcd(num, den) where num::Int32 and HOF inference for map(f, xs, ys).
                if param.is_varargs {
                    compiler
                        .julia_type_locals
                        .insert(param.name.clone(), JuliaType::Tuple);
                } else if param_ty.is_narrow_integer()
                    || matches!(param_ty, JuliaType::VectorOf(_) | JuliaType::MatrixOf(_))
                    || matches!(param_ty, JuliaType::Dict)
                    || matches!(&param_ty, JuliaType::Struct(name)
                        if !name.contains('{')
                            && compiler.shared_ctx.parametric_structs.contains_key(name))
                {
                    compiler
                        .julia_type_locals
                        .insert(param.name.clone(), param_ty.clone());
                }
                // Track parameters with TypeVar type annotations (e.g., x::T where T<:Integer)
                // so that variable references resolve to the bound type for proper
                // dispatch (Issue #2556).
                if let JuliaType::TypeVar(_, Some(bound_name)) = &param_ty {
                    if let Some(bound_type) = JuliaType::from_name(bound_name) {
                        if bound_type.is_abstract_numeric() {
                            // Abstract numeric bounds (`T<:Integer`, `T<:Real`, ...) accept many
                            // concrete runtime types (Int32, Int64, BigInt, ...). Storing the
                            // abstract bound in `julia_type_locals` would make calls such as
                            // `div(x, y)` statically dispatch to the generic `Any` fallback
                            // (`floor(x / y)` → Float64) instead of runtime-dispatching to the
                            // concrete integer method. Mirror a direct `x::Integer` annotation,
                            // which leaves the variable inferred as `Any` and relies on the
                            // `abstract_numeric_params` set plus runtime dispatch (Issue #5398).
                            compiler.abstract_numeric_params.insert(param.name.clone());
                            compiler
                                .julia_type_locals
                                .insert(param.name.clone(), param_ty.clone());
                        } else {
                            compiler
                                .julia_type_locals
                                .insert(param.name.clone(), bound_type.clone());
                        }
                    }
                }
                // Track parameters with Any type - these should preserve Any on reassignment
                if matches!(param_ty, JuliaType::Any) {
                    compiler.any_params.insert(param.name.clone());
                }
                // Track parameters with abstract numeric type annotations (Number, Real, etc.)
                // Binary operations on these must use runtime dispatch (Issue #2498)
                if param_ty.is_abstract_numeric() {
                    compiler.abstract_numeric_params.insert(param.name.clone());
                }
            }

            // Set up kwparam types in locals
            // For varargs kwargs (kwargs...), type is always NamedTuple
            // For required kwargs (Undef default), use type annotation if available
            // For unannotated optional kwargs, use Any since they can receive any type
            // at runtime regardless of the default's type (Issue #5425)
            for kwparam in &func.kwparams {
                let vt = if kwparam.is_varargs {
                    // Varargs kwargs collects all remaining kwargs as Pairs (Julia's Base.Pairs)
                    ValueType::Pairs
                } else {
                    let is_required = is_required_kwarg(&kwparam.default);
                    if is_required {
                        // Required kwarg - use type annotation if available
                        kwparam
                            .type_annotation
                            .as_ref()
                            .map(|jt| {
                                julia_type_to_value_type_with_table(
                                    jt,
                                    &compiler.shared_ctx.struct_table,
                                )
                            })
                            .unwrap_or(ValueType::Any)
                    } else {
                        // Every optional kwarg stays `Any` in the compiled body.
                        // Its default expression is only one possible runtime
                        // source and must not freeze typed loads/stores/returns:
                        // caller-supplied values may have any type accepted by
                        // the declared annotation (`x::Real = 1`, `x = 2.5`).
                        // The annotation remains an assertion at the kwsorter /
                        // entry boundary, not a slot representation. This mirrors
                        // the `KwParamInfo.ty` construction above (Issues #5425,
                        // #11024, #11135).
                        ValueType::Any
                    }
                };
                compiler.locals.insert(kwparam.name.clone(), vt);
                compiler.initialized_locals.insert(kwparam.name.clone());
            }

            // Register type parameters from where clause as DataType locals
            // This enables T(x) calls where T is a type parameter: function f(x::T) where T; T(1); end
            for tp in &normalized_type_params {
                // Skip Val{N} value parameters - they are I64, not DataType
                if !compiler.val_type_params.contains(&tp.name) {
                    compiler.locals.insert(tp.name.clone(), ValueType::DataType);
                    compiler.initialized_locals.insert(tp.name.clone());
                }
            }

            // Pre-populate locals with inferred types to ensure consistent type usage
            // This prevents bugs where a variable is first assigned as I64 then used as F64
            // Protect function parameters (and kwargs) from being overwritten by local assignments
            // This fixes the bug where parameter reassignment (e.g., a = abs(a)) causes type mismatch
            let protected: HashSet<String> = func
                .params
                .iter()
                .map(|p| p.name.clone())
                .chain(func.kwparams.iter().map(|k| k.name.clone()))
                .collect();
            collect_local_types_with_mixed_tracking(
                &func.body.stmts,
                &mut compiler.locals,
                &protected,
                &compiler.shared_ctx.struct_table,
                &compiler.shared_ctx.global_types,
                &mut compiler.mixed_type_vars,
            );

            // Compile function body with implicit return handling
            // In Julia, the last expression in a function is its return value.
            // Issue #5425 / #5466: when the body returns an unannotated optional kwarg
            // — directly (`g(; n = 0) = n`) or derived through a computation
            // (`g2(; n = 0) = n + 1`) — the runtime value can be any type, so emit the
            // body against `Any` to force `ReturnAny` instead of a typed return that
            // would reject a differently-typed result. `FunctionInfo`'s own
            // `return_type` stays precise for reflection (`Base.infer_return_type`).
            let body_return_type = if returns_unannotated_optional_kwparam_value(func)
                || returns_untyped_param_power_value(func)
            {
                ValueType::Any
            } else {
                function_infos[func_info_idx].return_type.clone()
            };
            // SSA pipeline gate (Issue #8552): eligible user function bodies
            // go Core IR → SSA build → opt passes → stack-bytecode lowering;
            // everything else (including all Base/prelude functions, so the
            // Base cache stays legacy-built) falls back per function.
            //
            // Issue #10115: a provably trivial body (single literal-arg call
            // or literal return, no control flow, no local assignment, no
            // `where`/kwparams) gets nothing from SSA construction + the
            // const-fold/CSE/DCE passes — there is nothing to fold,
            // eliminate, or reorder. Skip straight to the legacy path (the
            // same one every Base/prelude function already takes) instead of
            // paying for `build_function`/`optimize_scoped_resolved`/
            // `plan::plan_function` on a 1-statement graph.
            let shared_plan = if ssa_pipeline
                && is_user_function_scope
                && !compiler.in_base_function_scope
                && !is_trivial_ssa_fast_path_body(func)
            {
                let ssa_effects = ssa_effects.get_or_insert_with(|| {
                    // Name-level merge (the sound by-name dispatch fallback).
                    // `SJULIA_EFFECTS_STATS=1` additionally logs how much
                    // foldable/removable precision the name merge hides from a
                    // dispatch-resolving call site (Issue #9205 acceptance 3).
                    let by_name = if super::effects::propagation::effect_stats_logging_enabled() {
                        let summaries =
                            super::effects::propagation::infer_program_effects_per_method(program);
                        super::effects::propagation::log_per_method_precision_stats(&summaries);
                        summaries.by_name
                    } else {
                        super::effects::propagation::infer_program_effects(program)
                    };
                    // Static-dispatch resolver (Issue #9495): consult per-method
                    // summaries at statically-resolved call sites so a pure
                    // `f(::Int)` shadowed by an impure sibling is still
                    // foldable/removable. `None` (no fully-visible multi-method
                    // generic) leaves codegen byte-identical to the by-name path.
                    let resolver = super::effects::static_dispatch::StaticDispatchResolver::build(
                        program, &by_name,
                    );
                    SsaEffects { by_name, resolver }
                });
                let effects = &ssa_effects.by_name;
                let resolver = ssa_effects.resolver.as_ref();
                // `spec_func_mapping` membership means call sites emit
                // `CallSpecialize` and the VM specializer consumes this
                // body's slot-name table (Issue #8440 fallback condition).
                let runtime_specialized = compiler
                    .shared_ctx
                    .spec_func_mapping
                    .contains_key(&func_info_idx);
                super::ssa_ir::lower_function_body_via_ssa(
                    &mut compiler,
                    func,
                    body_return_type.clone(),
                    effects,
                    resolver,
                    runtime_specialized,
                )?
            } else {
                None
            };
            let lowered_via_ssa = shared_plan.is_some();
            if !lowered_via_ssa {
                compiler.compile_function_body(&func.body, body_return_type)?;
            }
            // Patch @goto jumps after function body compilation
            compiler.patch_goto_jumps()?;

            if hides_user_globals {
                for name in shadowed_user_globals {
                    compiler.shared_ctx.global_types.remove(name);
                }
            }

            let code_start = entry;
            let mut func_code = compiler.code;
            let func_source_map = compiler.source_map;
            relocate_jumps(&mut func_code, 0, entry);
            code.extend(func_code);
            source_map.extend(func_source_map);
            let code_end = code.len();

            // Update function boundaries for future caching
            {
                let fi = std::rc::Rc::make_mut(&mut function_infos[func_info_idx]);
                fi.code_start = code_start;
                fi.code_end = code_end;
                fi.shared_plan = shared_plan;
            }
        }
        for (name, ty) in &shadowed_global_types {
            if let Some(ty) = ty {
                shared_ctx.global_types.insert(name.clone(), ty.clone());
            }
        }
        profile::finish(emit_functions_timer);
        Ok(())
    }

    fn compile_inner_constructors(&mut self) -> CResult<()> {
        let mut bare_module_paths = HashSet::new();
        for module in &self.all_modules {
            collect_bare_module_paths(module, "", &mut bare_module_paths);
        }
        let toplevel_module_bindings = self.toplevel_module_bindings();
        let program = self.program;
        let inner_ctors = &self.inner_ctors;
        let method_tables = &self.method_tables;
        let module_functions = &self.module_functions;
        let module_exports = &self.module_exports;
        let imported_functions = &self.imported_functions;
        let module_imports_map = &self.module_imports_map;
        let module_usings_map = &self.module_usings_map;
        let usings_set = &self.usings_set;
        let abstract_type_names = &self.abstract_type_names;
        let module_constants = &self.module_constants;
        let function_infos = &mut self.function_infos;
        let reused_base = &mut self.reused_base;
        let code = &mut self.code;
        let source_map = &mut self.source_map;
        let shared_ctx = &mut self.shared_ctx;

        // Compile inner constructors
        // These run with current_struct_type_id set so new() creates the correct struct type
        let emit_inner_constructors_timer = profile::start("compile.emit_inner_constructors");
        for ctor_info in inner_ctors.iter() {
            let entry = code.len();
            let func_info_idx = ctor_info.func_info_idx;
            std::rc::Rc::make_mut(&mut function_infos[func_info_idx]).entry = entry;

            // Resolve the constructor body's name lookups in the struct's DEFINING
            // module, not at the call site. Upstream Julia always evaluates a
            // method body's names in its definition module, so a module-private
            // helper function, type, or const referenced inside an inner
            // constructor must be visible without the caller doing `using .Mod`
            // (Issue #8069). Mirror the module-scope setup that ordinary module
            // functions get in `compile_functions`.
            let module_path = ctor_info.module_path.as_deref();
            let mut ctor_imports = imported_functions.clone();
            if let Some(path) = module_path {
                if let Some(module_funcs) = module_functions.get(path) {
                    ctor_imports.extend(module_funcs.iter().cloned());
                }
                if let Some(module_imports) = module_imports_map.get(path) {
                    ctor_imports.extend(module_imports.iter().cloned());
                }
            }
            let ctor_scope_usings = module_path
                .and_then(|path| module_usings_map.get(path))
                .map(Vec::as_slice)
                .unwrap_or(program.usings.as_slice());
            let resolved_usings = resolve_scope_using_imports(
                ctor_scope_usings,
                module_path.unwrap_or(""),
                module_functions,
            );
            let normalized_type_params =
                shared_ctx.expand_type_param_bounds(&ctor_info.ctor.type_params);

            let mut compiler = CoreCompiler::new_for_function(
                method_tables,
                module_functions,
                module_exports,
                &ctor_imports,
                usings_set,
                resolved_usings,
                shared_ctx,
                abstract_type_names,
                module_constants,
            );
            compiler.toplevel_module_bindings = toplevel_module_bindings.clone();
            compiler.current_module_is_bare =
                module_path.is_some_and(|path| bare_module_paths.contains(path));

            // Resolve unqualified module-private struct names in the defining
            // module and keep cross-module dispatch pooling consistent with it,
            // exactly as `compile_functions` does for module methods (#8069).
            compiler.current_module_path = ctor_info.module_path.clone();
            compiler.current_module_imports = module_path
                .and_then(|path| module_imports_map.get(path))
                .cloned()
                .unwrap_or_default();

            // Carry the declaration-owned allocation target without a dummy id
            // or a bare-name projection. Only one compiler target can be active.
            let constructor_return_type = match &ctor_info.target {
                InnerCtorTarget::Concrete { type_id } => {
                    compiler.current_struct_type_id = Some(*type_id);
                    ValueType::Struct(*type_id)
                }
                InnerCtorTarget::Parametric { qualified_name } => {
                    compiler.current_parametric_struct_name = Some(qualified_name.clone());
                    ValueType::Any
                }
            };

            // Set type parameters from the constructor's where clause (e.g., where T)
            compiler.current_type_params = normalized_type_params.clone();
            compiler.current_type_param_index = normalized_type_params
                .iter()
                .enumerate()
                .map(|(i, tp)| (tp.name.clone(), i))
                .collect();
            if !ctor_info.is_base_origin {
                compiler.ctor_self_bound_type_vars = ctor_info
                    .ctor
                    .explicit_type_parameter_names
                    .iter()
                    .cloned()
                    .collect();
            }

            // Set up parameter types in locals
            for param in &ctor_info.ctor.params {
                let param_ty = param.effective_type();
                let vt = if matches!(param_ty, JuliaType::Dict) {
                    // See function-parameter setup above. (Issue #7632)
                    ValueType::Any
                } else {
                    compiler.julia_type_to_value_type_with_ctx(&param_ty)
                };
                compiler.locals.insert(param.name.clone(), vt);
                compiler.initialized_locals.insert(param.name.clone());
                // Track parameters with Any type - these should preserve Any on reassignment
                if matches!(param_ty, JuliaType::Any) {
                    compiler.any_params.insert(param.name.clone());
                }
                // Track parameters with abstract numeric type annotations (Issue #2498)
                if param_ty.is_abstract_numeric() {
                    compiler.abstract_numeric_params.insert(param.name.clone());
                }
            }

            // Determine which `where`-clause type parameters are recoverable from a
            // constructor argument at runtime (they appear in some parameter's type
            // annotation, e.g. `Bar(x::T)`). Only those can be safely materialized
            // by `new{...}` from the constructor frame; explicit-only parameters
            // (`Foo{T}(x)` with an untyped `x`) need call-site type-arg plumbing
            // that does not yet exist, so they fall back to the legacy runtime path
            // (Issue #5059).
            {
                let where_names: HashSet<&str> = ctor_info
                    .ctor
                    .type_params
                    .iter()
                    .map(|tp| tp.name.as_str())
                    .collect();
                for param in &ctor_info.ctor.params {
                    collect_referenced_type_var_names(
                        &param.effective_type(),
                        &where_names,
                        &mut compiler.ctor_arg_bound_type_vars,
                    );
                }
            }

            // Register type parameters from constructor's where clause as DataType locals
            // This enables T(x) calls inside inner constructors: function Foo{T}(x) where T; T(1); end
            for tp in &ctor_info.ctor.type_params {
                compiler.locals.insert(tp.name.clone(), ValueType::DataType);
                compiler.initialized_locals.insert(tp.name.clone());
            }

            // Protect constructor parameters from being overwritten by local assignments
            // This fixes the bug where parameter reassignment (e.g., num = div(num, g)) causes type mismatch
            let protected: HashSet<String> = ctor_info
                .ctor
                .params
                .iter()
                .map(|p| p.name.clone())
                .collect();
            collect_local_types_with_mixed_tracking(
                &ctor_info.ctor.body.stmts,
                &mut compiler.locals,
                &protected,
                &compiler.shared_ctx.struct_table,
                &compiler.shared_ctx.global_types,
                &mut compiler.mixed_type_vars,
            );

            // Compile constructor body
            compiler.compile_function_body(&ctor_info.ctor.body, constructor_return_type)?;
            // Patch @goto jumps after constructor body compilation
            compiler.patch_goto_jumps()?;

            let code_start = entry;
            let mut func_code = compiler.code;
            let func_source_map = compiler.source_map;
            relocate_jumps(&mut func_code, 0, entry);
            code.extend(func_code);
            source_map.extend(func_source_map);
            let code_end = code.len();

            // Update constructor function boundaries
            {
                let fi = std::rc::Rc::make_mut(&mut function_infos[func_info_idx]);
                fi.code_start = code_start;
                fi.code_end = code_end;
            }

            // Mark this inner constructor as not reused from cache (needs slot transformation)
            reused_base[func_info_idx] = false;
        }
        profile::finish(emit_inner_constructors_timer);
        Ok(())
    }

    fn compile_modules(&mut self) -> CResult<()> {
        // Record where modules start (this will be the entry point if there are modules)
        self.modules_entry = self.code.len();
        let all_modules = self.all_modules.clone();

        // Compile modules (execute their bodies before main)
        let emit_modules_timer = profile::start("compile.emit_modules");
        for module in all_modules {
            self.compile_module_recursive(module, &module.name)?;
        }
        profile::finish(emit_modules_timer);
        Ok(())
    }

    fn compile_module_recursive(
        &mut self,
        module: &crate::ir::core::Module,
        module_path: &str,
    ) -> CResult<()> {
        for submodule in &module.submodules {
            let submodule_path = format!("{}.{}", module_path, submodule.name);
            self.compile_module_recursive(submodule, &submodule_path)?;
        }

        // Find the __init__ function for this specific module before borrowing
        // self fields mutably in the inner block (Issue #8994).
        // We look up the all_funcs_idx first, then translate via func_index_map to get the
        // func_info_idx used by CallResolved. This is necessary because func_index_map is an
        // identity map only in the no-cache case; when a persistent/embedded base cache is
        // present, function_infos starts populated with cached_base_len entries which may
        // exceed base_function_count, so all_funcs_idx != func_info_idx for user/module
        // functions (Issue #8994).
        // Matching by (name == "__init__") AND (module_path == Some(path)) ensures
        // we call only this module's __init__, not a sibling module's overload.
        let init_func_info_idx: Option<usize> = module
            .functions
            .iter()
            .find(|f| f.name == "__init__")
            .and_then(|_| {
                self.all_functions
                    .iter()
                    .enumerate()
                    .find(|(_, (func, mp))| {
                        func.name == "__init__" && mp.as_deref() == Some(module_path)
                    })
                    .map(|(all_funcs_idx, _)| self.func_index_map[all_funcs_idx])
            });
        // Module-level function definitions are hoisted out of `module.body`
        // during lowering. Preserve their original evaluation positions so
        // definition-time signature checks still observe source-ordered
        // `using`/`import` activation (Issues #10396/#10582/#11419).
        let mut module_source_function_activations = module
            .functions
            .iter()
            .map(|func| {
                let func_info_idx = self
                    .all_functions
                    .iter()
                    .enumerate()
                    .find(|(_, (candidate, owner))| {
                        owner.as_deref() == Some(module_path)
                            && candidate.span.definition_order == func.span.definition_order
                            && candidate.name == func.name
                    })
                    .map(|(all_funcs_idx, _)| self.func_index_map[all_funcs_idx]);
                (
                    func.span.start,
                    func_info_idx,
                    func.type_params.clone(),
                    func.params.clone(),
                    func.kwparams.clone(),
                )
            })
            .collect::<Vec<_>>();
        module_source_function_activations.sort_by_key(|(start, ..)| *start);
        let toplevel_module_bindings = self.toplevel_module_bindings();

        {
            let module_offset = self.code.len();
            let module_imports_map = &self.module_imports_map;
            let method_tables = &self.method_tables;
            let module_functions = &self.module_functions;
            let module_exports = &self.module_exports;
            let imported_functions = &self.imported_functions;
            let usings_set = &self.usings_set;
            let abstract_type_names = &self.abstract_type_names;
            let module_constants = &self.module_constants;
            let code = &mut self.code;
            let source_map = &mut self.source_map;
            let shared_ctx = &mut self.shared_ctx;

            // Create module-local imported functions set: includes all functions defined in this module
            // and functions imported via `using` statements in this module
            let mut module_imported_functions = imported_functions.clone();
            for func in &module.functions {
                module_imported_functions.insert(func.name.clone());
            }

            // Add functions imported via module-local using statements
            if let Some(module_imports) = module_imports_map.get(module_path) {
                module_imported_functions.extend(module_imports.iter().cloned());
            }
            let resolved_usings =
                resolve_scope_using_imports(&module.usings, module_path, module_functions);
            let mut module_compiler = CoreCompiler::new(
                method_tables,
                module_functions,
                module_exports,
                &module_imported_functions,
                usings_set,
                resolved_usings,
                shared_ctx,
                abstract_type_names,
                module_constants,
            );
            module_compiler.enable_explicit_lexical_scopes();
            module_compiler.toplevel_module_bindings = toplevel_module_bindings;

            // Set module path for qualified constant storage
            module_compiler.current_module_path = Some(module_path.to_string());
            module_compiler.current_module_is_bare = module.is_bare;

            // Upstream publishes the module binding before evaluating its body:
            // an error leaves the module itself and the exact reached declaration
            // prefix visible. Record the committed owner path separately from
            // the ModuleValue's local display name for REPL recovery (#11761).
            module_compiler.emit(Instr::PushModule(Box::new(ModuleOperands {
                name: module.name.clone(),
                exports: module.exports.clone(),
                publics: module.publics.clone(),
                base_exports_visible: !module.is_bare
                    || module.usings.iter().any(|using_import| {
                        !using_import.is_import
                            && !using_import.is_relative
                            && using_import.module == "Base"
                            && using_import.symbols.is_none()
                    }),
                implicit_standard_bindings: !module.is_bare,
            })));
            module_compiler.emit(Instr::StoreAny(module.name.clone()));
            module_compiler.emit(Instr::ActivateModule(module_path.to_string()));

            // Package/include lowering can preserve `Module.usings` metadata
            // while omitting every executable marker from the module body. Seed
            // those imports at module entry. If even one marker survives, keep
            // the ordinary statement-driven chronology intact (Issues
            // #11203/#11216).
            if !module.usings.is_empty()
                && !module
                    .body
                    .stmts
                    .iter()
                    .any(|stmt| matches!(stmt, Stmt::Using { .. }))
            {
                module_compiler.compile_markerless_using_alias_activations()?;
            }

            // Compile the surviving module statements while replaying the
            // hoisted functions' definition-time signature probes at their
            // original source positions. A function before `using Base` in a
            // baremodule must still reject a Base-owned annotation, while the
            // same function after the import must accept it (Issue #11419).
            let mut activation_cursor = 0usize;
            let mut emit_pending_function_probes =
                |compiler: &mut CoreCompiler<'_>, before_start: usize| {
                    while let Some((
                        activation_start,
                        func_info_idx,
                        type_params,
                        params,
                        kwparams,
                    )) = module_source_function_activations.get(activation_cursor)
                    {
                        if *activation_start >= before_start {
                            break;
                        }
                        compiler.emit_hoisted_module_builtin_signature_probes(
                            type_params,
                            params,
                            kwparams,
                            *activation_start,
                        );
                        if let Some(func_info_idx) = func_info_idx {
                            compiler.emit(Instr::DefineFunction(*func_info_idx));
                        }
                        activation_cursor += 1;
                    }
                };
            for stmt in &module.body.stmts {
                emit_pending_function_probes(&mut module_compiler, stmt.span().start);
                module_compiler.compile_stmt(stmt)?;
            }
            emit_pending_function_probes(&mut module_compiler, usize::MAX);

            // Call __init__() immediately after the module body finishes, if defined.
            // Upstream Julia semantics: after all top-level code in a module has been
            // evaluated, `__init__()` is invoked automatically (see
            // `julia/base/loading.jl:run_module_init`). Submodules are compiled
            // (and their __init__ called) before the parent in compile_module_recursive,
            // so nested-module call order matches upstream (children before parent).
            // __init__ takes no arguments so it is never specializable; emit
            // CallResolved directly (bypassing emit_call_or_specialize) to avoid
            // any accidental CallSpecialize dispatch path. The return value is
            // discarded via Pop since __init__ results are always ignored (Issue #8994).
            if let Some(func_info_idx) = init_func_info_idx {
                module_compiler.emit(Instr::CallResolved(func_info_idx, 0));
                module_compiler.emit(Instr::Pop);
            }

            // Don't emit ReturnUnit - let execution flow through to next module or main

            // Patch @goto jumps after module body compilation
            module_compiler.patch_goto_jumps()?;

            let mut module_code = module_compiler.code;
            let module_source_map = module_compiler.source_map;
            relocate_jumps(&mut module_code, 0, module_offset);
            code.extend(module_code);
            source_map.extend(module_source_map);
        }

        Ok(())
    }

    fn compile_base_main_prefix(&mut self) -> CResult<()> {
        let program = self.program;
        let opt_main = self.opt_main;
        let user_function_names = &self.user_function_names;
        let method_tables = &self.method_tables;
        let module_functions = &self.module_functions;
        let module_exports = &self.module_exports;
        let imported_functions = &self.imported_functions;
        let usings_set = &self.usings_set;
        let abstract_type_names = &self.abstract_type_names;
        let module_constants = &self.module_constants;
        let code = &mut self.code;
        let source_map = &mut self.source_map;
        let shared_ctx = &mut self.shared_ctx;

        let stmts = &opt_main.stmts;
        let boundary_idx = stmts.iter().position(is_base_user_main_boundary);
        let (base_main_stmts, user_main_stmts) = if let Some(idx) = boundary_idx {
            (&stmts[..idx], &stmts[idx + 1..])
        } else {
            (&[][..], stmts.as_slice())
        };

        if boundary_idx.is_some() {
            let mut assigned_user_globals = HashSet::new();
            collect_assigned_binding_names(user_main_stmts, &mut assigned_user_globals);
            self.deferred_shadowed_global_types = assigned_user_globals
                .iter()
                .map(|name| {
                    let saved = shared_ctx.global_types.remove(name);
                    let restore = if user_function_names.contains(name)
                        && !assigned_user_globals.contains(name)
                    {
                        None
                    } else {
                        saved
                    };
                    (name.clone(), restore)
                })
                .collect();
        }

        if base_main_stmts.is_empty() {
            return Ok(());
        }

        let emit_base_main_timer = profile::start("compile.emit_base_main_prefix");
        let base_main_entry = code.len();
        self.base_main_entry = Some(base_main_entry);
        let resolved_usings = resolve_scope_using_imports(&program.usings, "", module_functions);

        let mut base_main_compiler = CoreCompiler::new(
            method_tables,
            module_functions,
            module_exports,
            imported_functions,
            usings_set,
            resolved_usings,
            shared_ctx,
            abstract_type_names,
            module_constants,
        );

        let protected: HashSet<String> = HashSet::new();
        collect_local_types_with_mixed_tracking(
            base_main_stmts,
            &mut base_main_compiler.locals,
            &protected,
            &base_main_compiler.shared_ctx.struct_table,
            &base_main_compiler.shared_ctx.global_types,
            &mut base_main_compiler.mixed_type_vars,
        );
        for stmt in base_main_stmts {
            base_main_compiler.compile_stmt(stmt)?;
        }
        base_main_compiler.patch_goto_jumps()?;

        let mut base_main_code = base_main_compiler.code;
        let base_main_source_map = base_main_compiler.source_map;
        relocate_jumps(&mut base_main_code, 0, base_main_entry);
        code.extend(base_main_code);
        source_map.extend(base_main_source_map);
        profile::finish(emit_base_main_timer);
        Ok(())
    }

    fn compile_main(&mut self) -> CResult<()> {
        let program = self.program;
        let opt_main = self.opt_main;
        let modules_entry = self.modules_entry;
        let all_modules = &self.all_modules;
        let method_tables = &self.method_tables;
        let module_functions = &self.module_functions;
        let module_exports = &self.module_exports;
        let imported_functions = &self.imported_functions;
        let usings_set = &self.usings_set;
        let abstract_type_names = &self.abstract_type_names;
        let module_constants = &self.module_constants;
        let stmts = &opt_main.stmts;
        let boundary_idx = stmts.iter().position(is_base_user_main_boundary);
        let user_main_stmts = if let Some(idx) = boundary_idx {
            (&stmts[..idx], &stmts[idx + 1..])
        } else {
            (&[][..], stmts.as_slice())
        }
        .1;
        let user_main_start = boundary_idx
            .and_then(|idx| stmts.get(idx).map(|stmt| stmt.span().start))
            .unwrap_or(opt_main.span.start);
        let runtime_nominal_scan =
            user_segment_opaque_runtime_eval(program, program.base_function_count);
        let runtime_nominal_chronology = runtime_nominal_scan.has_runtime_nominal;
        let runtime_nominal_names = &runtime_nominal_scan.runtime_nominal_names;
        let prelude_definition_order_max = crate::get_prelude_program()
            .and_then(|prelude| {
                prelude
                    .definition_order_bounds()
                    .map(|(_, maximum)| maximum)
            })
            .unwrap_or(0);
        let current_input_source_function_indices = repl_current_input_source_function_indices(
            &self.all_functions,
            self.first_user_function_idx,
            self.inline_start_idx,
            &self.func_idx_to_parent,
            self.repl_current_function_count,
        );
        // Hoisting removes top-level function definitions from `main`, so the
        // last executable statement is not necessarily the end of the user's
        // source fragment (and a definition-only input has no main statement at
        // all). Include the current input's source-function spans in the boundary
        // used to select activation markers; otherwise trailing definitions are
        // silently omitted and can only become visible through an unsound eager
        // installer side effect (Issues #9784/#11477).
        let current_input_function_span_end = current_input_source_function_indices
            .as_ref()
            .and_then(|current_indices| {
                self.all_functions
                    .iter()
                    .enumerate()
                    .filter(|(all_funcs_idx, (_, module_path))| {
                        *all_funcs_idx >= self.first_user_function_idx
                            && *all_funcs_idx < self.inline_start_idx
                            && current_indices.contains(all_funcs_idx)
                            && module_path.is_none()
                            && !self.func_idx_to_parent.contains_key(all_funcs_idx)
                    })
                    .map(|(_, (func, _))| func.span.end)
                    .max()
            })
            .unwrap_or(0);
        let current_input_struct_keys = self
            .repl_current_struct_count
            .map(|count| {
                self.all_structs
                    .iter()
                    .filter(|(_, module_path, inherited)| module_path.is_none() && !*inherited)
                    .take(count)
                    .filter(|(def, ..)| !def.is_parametric())
                    .map(|(def, ..)| (def.name.clone(), def.span.definition_order, def.span.start))
                    .collect::<HashSet<_>>()
            })
            .or_else(|| {
                runtime_nominal_chronology.then(|| {
                    self.all_structs
                        .iter()
                        .filter(|(def, module_path, inherited)| {
                            module_path.is_none()
                                && !*inherited
                                && !def.is_parametric()
                                && !runtime_nominal_names.contains(&def.name)
                                && def.span.definition_order != 0
                                && def.span.start >= user_main_start
                                && def.span.end <= opt_main.span.end
                        })
                        .map(|(def, ..)| {
                            (def.name.clone(), def.span.definition_order, def.span.start)
                        })
                        .collect::<HashSet<_>>()
                })
            })
            .unwrap_or_default();
        let is_current_input_struct =
            |def: &crate::ir::core::StructDef, module_path: &Option<String>, inherited: bool| {
                module_path.is_none()
                    && !inherited
                    && !def.is_parametric()
                    && current_input_struct_keys.contains(&(
                        def.name.clone(),
                        def.span.definition_order,
                        def.span.start,
                    ))
            };
        let current_input_struct_span_end = self
            .all_structs
            .iter()
            .filter(|(def, module_path, inherited)| {
                is_current_input_struct(def, module_path, *inherited)
                    && self
                        .shared_ctx
                        .type_definition_positions
                        .get(&def.name)
                        .is_some_and(|position| {
                            position.definition_order == def.span.definition_order
                                && position.source_start == def.span.start
                        })
            })
            .map(|(def, ..)| def.span.end)
            .max()
            .unwrap_or(0);
        // Issue #11118: `current_input_function_span_end` / `current_input_struct_span_end`
        // only widen the bound for a REPL delta (both stay 0 when
        // `repl_current_function_count`/`repl_current_struct_count` are `None`), so a
        // plain single-shot compile whose ENTIRE user body is declarations -- or whose
        // last non-declaration statement is followed by a trailing declaration, e.g.
        // `g(x::LaterDefined) = 1` alone in the file, or
        // `println(1); g(x::LaterDefined) = 1` -- still fell back to
        // `user_main_stmts.last()` (or 0). Widen with `opt_main.span.end` too: it is the
        // whole user source fragment's own span end (`0..source.len()` for a freshly
        // lowered file), so it always dominates any declaration's span within that same
        // input regardless of what, if anything, follows it. Safe to combine with the
        // REPL-specific bounds above: inclusion into `top_level_definition_activations`
        // below also requires an INDEX-based current-input test, which alone already
        // excludes any definition outside the current input -- widening this span bound
        // can only admit a same-input trailing/only declaration that a too-tight span
        // previously excluded, never a stale one.
        let user_main_end = user_main_stmts
            .last()
            .map(|stmt| stmt.span().end)
            .unwrap_or(0)
            .max(current_input_function_span_end)
            .max(current_input_struct_span_end)
            .max(opt_main.span.end);
        let activate_markerless_main_usings = self.extra_imported_functions.is_none()
            && !program.usings.is_empty()
            && !user_main_stmts
                .iter()
                .any(|stmt| matches!(stmt, Stmt::Using { .. }));
        // A function nested inside an untaken `if`/zero-iteration `while`/`for`
        // branch is NOT unconditionally reached just because its source
        // position precedes a later top-level statement; excluding it here
        // leaves its `DefineEvalFunction` activation solely to `compile_stmt`'s
        // own correctly branch-gated emission (Issue #11320).
        let mut conditionally_gated_function_starts: HashSet<usize> = HashSet::new();
        collect_conditionally_gated_function_starts(
            user_main_stmts,
            false,
            &mut conditionally_gated_function_starts,
        );
        let mut top_level_definition_activations = self
            .all_functions
            .iter()
            .enumerate()
            .filter_map(|(all_funcs_idx, (func, module_path))| {
                let is_top_level_user_function = all_funcs_idx >= self.first_user_function_idx
                    && all_funcs_idx < self.inline_start_idx
                    && module_path.is_none()
                    && !self.func_idx_to_parent.contains_key(&all_funcs_idx);
                let is_current_input_top_level_user_function = is_top_level_user_function
                    && current_input_source_function_indices
                        .as_ref()
                        .is_none_or(|indices| indices.contains(&all_funcs_idx))
                    && !crate::compile::ir_inline::is_markerless_lowered_function(func);
                let is_source_ordered_inline_function = all_funcs_idx >= self.inline_start_idx
                    && module_path.is_none()
                    && !self.func_idx_to_parent.contains_key(&all_funcs_idx)
                    && !crate::compile::ir_inline::is_markerless_lowered_function(func);
                ((is_current_input_top_level_user_function || is_source_ordered_inline_function)
                    && func.span.start >= user_main_start
                    && func.span.end <= user_main_end
                    && !conditionally_gated_function_starts.contains(&func.span.start))
                .then(|| TopLevelDefinitionActivation::Function {
                    source_start: func.span.start,
                    func_idx: self.func_index_map[all_funcs_idx],
                    // Issue #10396 / #10582: `where`-clause params and
                    // parameter annotations whose type names must
                    // resolve when this hoisted top-level definition
                    // activates (see
                    // `emit_signature_definition_probes`).
                    type_params: func.type_params.clone(),
                    params: func.params.clone(),
                    kwparams: func.kwparams.clone(),
                    // Issue #11025: source-order ordinal of THIS definition,
                    // compared against each annotation type's own ordinal so a
                    // forward reference still probes.
                    definition_order: func.span.definition_order,
                })
            })
            .collect::<Vec<_>>();
        top_level_definition_activations.extend(self.all_structs.iter().filter_map(
            |(def, module_path, inherited)| {
                let is_current_main_concrete =
                    is_current_input_struct(def, module_path, *inherited)
                        && def.span.start >= user_main_start
                        && def.span.end <= user_main_end
                        && self
                            .shared_ctx
                            .type_definition_positions
                            .get(&def.name)
                            .is_some_and(|position| {
                                position.definition_order == def.span.definition_order
                                    && position.source_start == def.span.start
                            });
                if !is_current_main_concrete {
                    return None;
                }
                let (_, info) = self
                    .shared_ctx
                    .struct_table
                    .resolve_scoped(&def.name, None, true)?;
                Some(TopLevelDefinitionActivation::Struct {
                    source_start: def.span.start,
                    definition_order: def.span.definition_order,
                    type_name: def.name.clone(),
                    type_id: info.type_id,
                })
            },
        ));
        if self.repl_current_function_count.is_some() || runtime_nominal_chronology {
            let abstract_type_indices = self
                .abstract_types
                .iter()
                .enumerate()
                .map(|(index, definition)| (definition.name.as_str(), index))
                .collect::<HashMap<_, _>>();
            top_level_definition_activations.extend(
                program
                    .abstract_types
                    .iter()
                    .filter(|definition| {
                        definition.span.start >= user_main_start
                            && definition.span.end <= user_main_end
                            && !runtime_nominal_names.contains(&definition.name)
                            && (self.repl_current_function_count.is_some()
                                || definition.span.definition_order > prelude_definition_order_max)
                    })
                    .filter_map(|definition| {
                        abstract_type_indices
                            .get(definition.name.as_str())
                            .map(|type_id| TopLevelDefinitionActivation::AbstractType {
                                source_start: definition.span.start,
                                definition_order: definition.span.definition_order,
                                type_name: definition.name.clone(),
                                type_id: *type_id,
                            })
                    }),
            );
            let primitive_type_indices = self
                .primitive_types
                .iter()
                .enumerate()
                .map(|(index, definition)| (definition.name.as_str(), index))
                .collect::<HashMap<_, _>>();
            top_level_definition_activations.extend(
                program
                    .primitive_types
                    .iter()
                    .filter(|definition| {
                        definition.span.start >= user_main_start
                            && definition.span.end <= user_main_end
                            && !runtime_nominal_names.contains(&definition.name)
                            && (self.repl_current_function_count.is_some()
                                || definition.span.definition_order > prelude_definition_order_max)
                    })
                    .filter_map(|definition| {
                        primitive_type_indices
                            .get(definition.name.as_str())
                            .map(|type_id| TopLevelDefinitionActivation::PrimitiveType {
                                source_start: definition.span.start,
                                definition_order: definition.span.definition_order,
                                type_name: definition.name.clone(),
                                type_id: *type_id,
                            })
                    }),
            );
        }
        let repl_definition_chronology =
            self.repl_current_function_count.is_some() || runtime_nominal_chronology;
        top_level_definition_activations.sort_by(|left, right| {
            let left_order = left.definition_order();
            let right_order = right.definition_order();
            // REPL current-input functions and structs share one monotonic
            // definition chronology. Ordinary whole-program compilation has
            // no struct markers and still flushes against byte offsets; keep
            // its established offset order because include-file offsets are
            // local and therefore incomparable across independently parsed
            // files (Issue #11546).
            let chronology = if repl_definition_chronology && left_order != 0 && right_order != 0 {
                left_order.cmp(&right_order)
            } else {
                left.source_start().cmp(&right.source_start())
            };
            chronology
                .then_with(|| left.source_start().cmp(&right.source_start()))
                .then_with(|| left.kind_rank().cmp(&right.kind_rank()))
        });
        let toplevel_module_bindings = self.toplevel_module_bindings();
        let code = &mut self.code;
        let source_map = &mut self.source_map;
        let shared_ctx = &mut self.shared_ctx;

        // Compile main block
        let emit_main_timer = profile::start("compile.emit_main");
        let main_entry = code.len();
        // Record where the USER main begins (Issue #9199 LV2). `entry` below
        // points at the base-main prefix so Base initializers run first; this is
        // the start of the user's own top-level code, the slice boundary the REPL
        // live-append path extracts as the relocatable delta main. Tracked through
        // the peephole passes in `finalize`, mirroring `entry`.
        self.user_main_entry = Some(main_entry);
        // Entry point: Base top-level initializers must run before user modules,
        // because module bodies may call Base functions whose internal constants
        // are initialized in the Base main prefix (Issue #7570).
        self.entry = if let Some(base_main_entry) = self.base_main_entry {
            base_main_entry
        } else if !all_modules.is_empty() {
            modules_entry
        } else {
            main_entry
        };
        let resolved_usings = resolve_scope_using_imports(&program.usings, "", module_functions);
        let mut main_compiler = CoreCompiler::new(
            method_tables,
            module_functions,
            module_exports,
            imported_functions,
            usings_set,
            resolved_usings,
            shared_ctx,
            abstract_type_names,
            module_constants,
        );
        main_compiler.enable_explicit_lexical_scopes();
        if let Some(global_slot_seed) = self.global_slot_seed {
            main_compiler
                .preexisting_global_bindings
                .extend(global_slot_seed.iter().cloned());
        }
        main_compiler.repl_source_ordered_top_level_dispatch =
            self.repl_current_function_count.is_some();
        main_compiler.repl_source_ordered_type_names = top_level_definition_activations
            .iter()
            .filter_map(|activation| match activation {
                TopLevelDefinitionActivation::Struct { type_name, .. }
                | TopLevelDefinitionActivation::AbstractType { type_name, .. }
                | TopLevelDefinitionActivation::PrimitiveType { type_name, .. } => {
                    Some(type_name.clone())
                }
                TopLevelDefinitionActivation::Function { .. } => None,
            })
            .collect();
        main_compiler.toplevel_module_bindings = toplevel_module_bindings;
        let protected: HashSet<String> = HashSet::new();
        for (name, ty) in self.deferred_shadowed_global_types.drain(..) {
            if let Some(ty) = ty {
                main_compiler.shared_ctx.global_types.insert(name, ty);
            }
        }

        // Pre-populate user-main locals only after Base main has compiled. Scanning
        // the merged Base+user block at once lets a user binding like `idx = [...]`
        // change the static type of Base's own `idx` temporaries before those Base
        // statements compile (Issue #5590).
        collect_local_types_with_mixed_tracking(
            user_main_stmts,
            &mut main_compiler.locals,
            &protected,
            &main_compiler.shared_ctx.struct_table,
            &main_compiler.shared_ctx.global_types,
            &mut main_compiler.mixed_type_vars,
        );

        // Synthetic macro/package programs can retain top-level import
        // metadata without executable markers. Mirror module compilation's
        // markerless entry activation so imported helpers exist before main
        // executes. REPL deltas carry `extra_imported_functions` and reuse the
        // live/prefix activation state instead (Issue #11251).
        if activate_markerless_main_usings {
            main_compiler.compile_markerless_using_alias_activations()?;
        }

        // Drains every top-level function/struct/type-annotation signature probe
        // still queued in `top_level_definition_activations` whose own source
        // position is before `before_start` (`usize::MAX` drains everything
        // that remains). Function/struct definitions are hoisted out of
        // `user_main_stmts`, so source-order activation markers are interleaved
        // by span rather than by statement. Keep the cursor outside the
        // non-empty-main branch below and share it with the FINAL flush after
        // the whole match (Issues #9784/#11477/#11118): a definition-only input
        // has no main statements but must still activate every source definition
        // before returning, and a definition can only ever be probed once.
        let mut activation_cursor = 0usize;
        let mut emit_pending_definition_activations =
            |compiler: &mut CoreCompiler<'_>, before_start: usize| {
                while let Some(activation) = top_level_definition_activations.get(activation_cursor)
                {
                    // Serialized/synthetic Core IR may assign the same coarse
                    // span to a Program function and the main statement that
                    // calls it. Program.functions precede main by construction,
                    // so equality belongs on the reached side of this boundary.
                    // (The `usize::MAX` final-drain flush below never has
                    // `activation.source_start() == usize::MAX` in practice, so
                    // `>` vs `>=` makes no difference there — every remaining
                    // activation still drains.)
                    if activation.source_start() > before_start {
                        break;
                    }
                    // Issue #10396: upstream evaluates function-signature
                    // `where`-bounds eagerly when the definition executes —
                    // `h2(x::T) where T<:UndefZZZ = x` must raise
                    // UndefVarError here, before the method activates and
                    // before any later statement runs. Issue #10582: the
                    // same eager evaluation applies to parameter-annotation
                    // type names (`f(x::SomeUndefName) = 1`). (No-op if the
                    // definition was already probed by a `Stmt::FunctionDef`
                    // inside an enclosing block statement — the span-keyed
                    // dedupe keeps a caught error from re-raising here.)
                    match activation {
                        TopLevelDefinitionActivation::Function {
                            source_start,
                            definition_order,
                            func_idx,
                            type_params,
                            params,
                            kwparams,
                        } => {
                            compiler.emit_signature_definition_probes(
                                type_params,
                                params,
                                kwparams,
                                *source_start,
                                *definition_order,
                            );
                            compiler.emit_eval_function_activation_once(*func_idx);
                        }
                        TopLevelDefinitionActivation::Struct { type_id, .. } => {
                            compiler.emit(Instr::DefineEvalStruct(*type_id));
                        }
                        TopLevelDefinitionActivation::AbstractType { type_id, .. } => {
                            compiler.emit(Instr::DefineEvalAbstractType(*type_id));
                        }
                        TopLevelDefinitionActivation::PrimitiveType { type_id, .. } => {
                            compiler.emit(Instr::DefineEvalPrimitiveType(*type_id));
                        }
                    }
                    activation_cursor += 1;
                }
            };
        // Compile all statements except the last one.
        if !user_main_stmts.is_empty() {
            for stmt in &user_main_stmts[..user_main_stmts.len() - 1] {
                emit_pending_definition_activations(&mut main_compiler, stmt.span().start);
                main_compiler.compile_stmt(stmt)?;
            }

            // For the last statement, if it's an expression, return its value
            // In Julia, assignment is also an expression that returns the assigned value
            let last_stmt = &user_main_stmts[user_main_stmts.len() - 1];
            emit_pending_definition_activations(&mut main_compiler, last_stmt.span().start);
            match last_stmt {
                Stmt::Expr { expr, .. } => {
                    let ty = main_compiler.compile_expr(expr)?;
                    main_compiler.emit_return_for_type(ty);
                }
                // Assignment as last statement returns the assigned value (Julia semantics)
                Stmt::Assign { var, value, .. } => {
                    if main_compiler.const_bindings.contains(var)
                        && !main_compiler.pending_const_bindings.remove(var)
                        && !main_compiler.strict_undefined_check
                    {
                        main_compiler.emit(Instr::PushStr(format!(
                            "invalid assignment to constant Main.{}",
                            var
                        )));
                        main_compiler.emit(Instr::ThrowError);
                        main_compiler.emit_return_for_type(ValueType::Nothing);
                    } else {
                        let was_pending_const = main_compiler.pending_const_bindings.remove(var);
                        let folded_const_value =
                            if was_pending_const && !main_compiler.strict_undefined_check {
                                crate::compile::const_prop::fold_expr_const_value(value, &|name| {
                                    main_compiler.const_values.get(name).cloned()
                                })
                            } else {
                                None
                            };
                        // Check for wider type as in compile_stmt
                        let target_ty = main_compiler.locals.get(var).cloned();
                        let ty = main_compiler.compile_expr(value)?;

                        // Handle widening for consistency with compile_stmt
                        // For mixed-type variables, use dynamic typing (don't convert I64 to F64)
                        let is_mixed_type = main_compiler.mixed_type_vars.contains(var);
                        let final_ty = match (target_ty, ty.clone()) {
                            // For mixed-type variables, preserve the actual type
                            (Some(ValueType::Any), ValueType::I64)
                            | (Some(ValueType::Any), ValueType::F64)
                                if is_mixed_type =>
                            {
                                ValueType::Any
                            }
                            (Some(target), incoming)
                                if is_mixed_type
                                    && !static_assignment_types_compatible(&target, &incoming) =>
                            {
                                ValueType::Any
                            }
                            (Some(ValueType::F64), ValueType::I64) if is_mixed_type => ty,
                            (Some(ValueType::I64), ValueType::F64) if is_mixed_type => ty,
                            // For non-mixed variables, apply widening
                            (Some(ValueType::F64), ValueType::I64) => {
                                main_compiler.emit(Instr::ToF64);
                                ValueType::F64
                            }
                            _ => ty,
                        };

                        // Duplicate the value before storing (for supported types)
                        // For other types, store and then load back
                        let needs_load_back = !matches!(final_ty, ValueType::I64 | ValueType::F64);

                        if !needs_load_back {
                            // For I64 and F64, we have Dup instructions
                            let dup_instr = match final_ty {
                                ValueType::I64 => Instr::DupI64,
                                ValueType::F64 => Instr::DupF64,
                                _ => {
                                    return err(format!(
                                        "internal: unexpected type {:?} in Dup path",
                                        final_ty
                                    ))
                                }
                            };
                            main_compiler.emit(dup_instr);
                            main_compiler.store_local(var, final_ty.clone());
                        } else {
                            // For other types, store first then load back
                            main_compiler.store_local(var, final_ty.clone());
                            main_compiler.load_local(var)?;
                        }
                        if was_pending_const && !main_compiler.strict_undefined_check {
                            main_compiler.const_bindings.insert(var.clone());
                            if let Some(value) = folded_const_value {
                                main_compiler.const_values.insert(var.clone(), value);
                            } else {
                                main_compiler.const_values.remove(var);
                            }
                        } else if !main_compiler.const_bindings.contains(var) {
                            main_compiler.const_values.remove(var);
                        }

                        main_compiler.emit_return_for_type(final_ty);
                    }
                }
                Stmt::Block(block) => {
                    // `y = begin...end` is lowered as a Stmt::Block where the
                    // last element is a Stmt::Assign that stores the block
                    // value to `y` (Issue #8977). Compile the prefix stmts
                    // normally, then recurse into the block's last statement
                    // so it gets the same last-stmt return semantics as a
                    // top-level statement (Dup-store-return for I64/F64,
                    // store-load for other types).
                    if block.stmts.is_empty() {
                        main_compiler.emit(Instr::ReturnNothing);
                    } else {
                        let (prefix, inner_last) = block.stmts.split_at(block.stmts.len() - 1);
                        for stmt in prefix {
                            main_compiler.compile_stmt(stmt)?;
                        }
                        let inner_last = &inner_last[0];
                        match inner_last {
                            Stmt::Expr { expr, .. } => {
                                let ty = main_compiler.compile_expr(expr)?;
                                main_compiler.emit_return_for_type(ty);
                            }
                            Stmt::Assign { var, value, .. } => {
                                if main_compiler.const_bindings.contains(var)
                                    && !main_compiler.pending_const_bindings.remove(var)
                                    && !main_compiler.strict_undefined_check
                                {
                                    main_compiler.emit(Instr::PushStr(format!(
                                        "invalid assignment to constant Main.{}",
                                        var
                                    )));
                                    main_compiler.emit(Instr::ThrowError);
                                    main_compiler.emit_return_for_type(ValueType::Nothing);
                                } else {
                                    let was_pending_const =
                                        main_compiler.pending_const_bindings.remove(var);
                                    let folded_const_value = if was_pending_const
                                        && !main_compiler.strict_undefined_check
                                    {
                                        crate::compile::const_prop::fold_expr_const_value(
                                            value,
                                            &|name| main_compiler.const_values.get(name).cloned(),
                                        )
                                    } else {
                                        None
                                    };
                                    let target_ty = main_compiler.locals.get(var).cloned();
                                    let ty = main_compiler.compile_expr(value)?;
                                    let is_mixed_type = main_compiler.mixed_type_vars.contains(var);
                                    let final_ty = match (target_ty, ty.clone()) {
                                        (Some(ValueType::Any), ValueType::I64)
                                        | (Some(ValueType::Any), ValueType::F64)
                                            if is_mixed_type =>
                                        {
                                            ValueType::Any
                                        }
                                        (Some(target), incoming)
                                            if is_mixed_type
                                                && !static_assignment_types_compatible(
                                                    &target, &incoming,
                                                ) =>
                                        {
                                            ValueType::Any
                                        }
                                        (Some(ValueType::F64), ValueType::I64) if is_mixed_type => {
                                            ty
                                        }
                                        (Some(ValueType::I64), ValueType::F64) if is_mixed_type => {
                                            ty
                                        }
                                        (Some(ValueType::F64), ValueType::I64) => {
                                            main_compiler.emit(Instr::ToF64);
                                            ValueType::F64
                                        }
                                        _ => ty,
                                    };
                                    let needs_load_back =
                                        !matches!(final_ty, ValueType::I64 | ValueType::F64);
                                    if !needs_load_back {
                                        let dup_instr = match final_ty {
                                            ValueType::I64 => Instr::DupI64,
                                            ValueType::F64 => Instr::DupF64,
                                            _ => {
                                                return err(format!(
                                                    "internal: unexpected type {:?} in Dup path",
                                                    final_ty
                                                ))
                                            }
                                        };
                                        main_compiler.emit(dup_instr);
                                        main_compiler.store_local(var, final_ty.clone());
                                    } else {
                                        main_compiler.store_local(var, final_ty.clone());
                                        main_compiler.load_local(var)?;
                                    }
                                    if was_pending_const && !main_compiler.strict_undefined_check {
                                        main_compiler.const_bindings.insert(var.clone());
                                        if let Some(value) = folded_const_value {
                                            main_compiler.const_values.insert(var.clone(), value);
                                        } else {
                                            main_compiler.const_values.remove(var);
                                        }
                                    } else if !main_compiler.const_bindings.contains(var) {
                                        main_compiler.const_values.remove(var);
                                    }
                                    main_compiler.emit_return_for_type(final_ty);
                                }
                            }
                            other => {
                                main_compiler.compile_stmt(other)?;
                                main_compiler.emit(Instr::ReturnNothing);
                            }
                        }
                    }
                }
                other => {
                    main_compiler.compile_stmt(other)?;
                    main_compiler.emit(Instr::ReturnNothing);
                }
            }
        } else {
            // Issue #11118: a top-level program whose ENTIRE user body is
            // declarations (e.g. a bare `g(x::LaterDefined) = 1` with no other
            // statement in the file at all — the exact shape of a program
            // consisting solely of `abstract type`/`struct`/function
            // definitions) never enters the branch above, so the per-statement
            // flush loop that emits `emit_signature_definition_probes` never
            // runs even once. Upstream Julia still evaluates that lone
            // method's signature annotations eagerly (raising `UndefVarError`
            // for a genuine forward reference) even though it is the only
            // statement in the file. The FINAL flush below (shared with every
            // other exit path) still drains every activation queued so far
            // before returning, so this shape gets the same eager evaluation
            // as a program with trailing executable code.
            main_compiler.emit(Instr::ReturnNothing);
        }

        // Every last-statement branch leaves exactly one return instruction at
        // the tail. Temporarily remove it, publish definitions whose source span
        // follows the last executable statement, then restore the return. This
        // preserves the last expression's value on the operand stack while
        // ensuring a trailing or definition-only method becomes visible only
        // after execution reaches its source position. An exception before this
        // tail naturally skips the markers and leaves the suffix dormant.
        let Some(trailing_return) = main_compiler.code.pop() else {
            return err("internal: compiled REPL main has no trailing return".to_string());
        };
        let trailing_return_span = main_compiler.source_map.pop().unwrap_or(None);
        emit_pending_definition_activations(&mut main_compiler, usize::MAX);
        main_compiler.code.push(trailing_return);
        main_compiler.source_map.push(trailing_return_span);

        // Patch @goto jumps after main code compilation
        main_compiler.patch_goto_jumps()?;

        // Snapshot which names are still genuinely main-scope bindings once the
        // whole main block has compiled (Issue #9157). A `let`/`@testset` block
        // restores `initialized_locals` to its pre-block value on exit, so a
        // name it introduces (never assigned outside the block) is absent here.
        self.main_scope_names = main_compiler.initialized_locals.clone();

        let mut main_code = main_compiler.code;
        let main_source_map = main_compiler.source_map;
        // Use main_entry (where main code actually starts) instead of entry (modules_entry)
        // for jump relocation. This ensures jumps point to correct addresses when modules exist.
        relocate_jumps(&mut main_code, 0, main_entry);
        code.extend(main_code);
        source_map.extend(main_source_map);
        profile::finish(emit_main_timer);
        Ok(())
    }

    fn finalize(
        self,
        inference_engine: &abstract_interp::InferenceEngine,
    ) -> CResult<CoreCompileOutput> {
        let CorePipeline {
            program,
            precompiled_base,
            base_function_count,
            shared_ctx,
            abstract_types,
            primitive_types,
            method_tables,
            mut function_infos,
            show_methods,
            print_methods,
            specializable_functions,
            reused_base,
            code,
            source_map,
            entry,
            mut module_registry,
            module_functions,
            imported_functions,
            main_scope_names,
            all_functions,
            all_modules,
            // Issue #9254: the preload-cache layout now spans the whole non-Base
            // region (`base_function_count..`), so `first_user_function_idx` no
            // longer bounds `nonbase_layout` here.
            first_user_function_idx: _,
            func_index_map,
            preload_reused,
            user_main_entry,
            global_slot_seed,
            cached_base_extra_reused_count,
            ..
        } = self;

        // Issue #9189: record which `function_infos` index each module-scoped
        // function ended up at, alongside its bare (unqualified) IR name.
        // `func_index_map` already carries the index mapping (all_functions
        // index -> function_infos index) for every function, so this is a
        // cheap re-projection, not new analysis. Used only by the (currently
        // unwired) preloaded-package cache generator.
        let mut module_function_infos: HashMap<String, Vec<(usize, String)>> = HashMap::new();
        for (all_idx, (func, module_path)) in all_functions.iter().enumerate() {
            if let Some(path) = module_path {
                if let Some(&func_info_idx) = func_index_map.get(all_idx) {
                    module_function_infos
                        .entry(path.clone())
                        .or_default()
                        .push((func_info_idx, func.name.clone()));
                }
            }
        }

        // Issue #9230/#9245/#9254: the FULL non-Base function layout —
        // `all_functions[base_function_count..]` in global-function-index order —
        // for the preload cache's layout-identity gate (see
        // `CoreCompileOutput::nonbase_layout`). A spliced module body's frozen
        // absolute call targets reference Base functions (`< base_function_count`,
        // always aligned via the Base cache) AND non-Base functions/closures —
        // NOT only the package region. In particular they reach the trailing
        // lifted Base closures (`_rstrip_eq_pred`, broadcast `fused`/`sel`, the
        // `__lambda_nested_*` predicates, …) that the two-region split (#9245)
        // leaves after the package region. #9245 narrowed this layout to
        // `[base_function_count..first_user_function_idx]` (package region only)
        // on the assumption that spliced bodies never reach those trailing
        // closures — but they do. A user/main lifted lambda (the #9254 iOS
        // Surface sample's `(x, y) -> …` argument) then interposed at the FRONT
        // of the trailing block and shifted every trailing Base closure by one,
        // so a frozen index that resolved to `_rstrip_eq_pred` at cache-generation
        // time pointed one slot off at consumption and surface silently degraded
        // to a 2-D line. Capturing the WHOLE non-Base region restores true layout
        // identity: the gate now deactivates (fail-safe wholesale compile)
        // whenever ANY user function or lifted lambda shifts the region, and stays
        // active for programs that add nothing after the deterministic Base-closure
        // tail (e.g. `plot([1, 2, 3])`, `plot(sin)`). Generation compiles
        // `using P1\nusing P2\n…` with no user code, so its captured layout is
        // exactly `[package region][deterministic trailing Base closures]`.
        // `all_functions` order matches `function_infos` order (one FunctionInfo
        // per entry).
        let nonbase_layout: Vec<(Option<String>, String)> = all_functions
            .iter()
            .skip(base_function_count)
            .map(|(func, module_path)| (module_path.clone(), func.name.clone()))
            .collect();
        // Keep cached Base bytecode out of the mutable suffix while compiling
        // (Issue #6348): the prefix is prepended only here, after the user/main
        // suffix has been optimized and slotized.
        //
        // This is also the mechanism that satisfies Issue #10117 ("skip
        // peephole+slotize on pre-compiled Base bytecode"): the peephole
        // passes below (`compile.peephole_pre_slotize`/`compile.slotize`/
        // `compile.peephole_post_slotize`) run on `code`, which at this point
        // is ONLY the freshly emitted non-Base suffix — the cached Base
        // bytecode (`base_code_prefix`) is not spliced in until
        // `compile.cached_code_prefix_assemble` below, strictly after all
        // three passes finish. So the Base portion is never re-scanned by
        // peephole/slotize on a cache-hit compile; profiling
        // `println("Hello World")` shows all three passes combined at
        // ~0.2ms total, already on the same order as the issue's proposed
        // `optimize_with_boundaries`-based target. `optimize_with_boundaries`
        // (subset_julia_vm_bytecode/src/stack_backend.rs) is still used above
        // for the REPL LV2 seam barrier (`seam_barrier_pre`/`seam_barrier_post`),
        // a narrower case (protecting a splice seam WITHIN the non-Base
        // suffix, not excluding Base).
        let base_code_prefix = precompiled_base.map(|base_cache| base_cache.code.as_slice());
        let base_code_prefix_len = base_code_prefix.map_or(0, <[_]>::len);

        // LV2 (Issue #9199): for the relocatable-delta compile (seed present),
        // install a peephole fusion barrier at the base-main / user-main seam so
        // the compiled user main stays a self-contained block sliceable out of
        // the buffer. `user_main_entry` here is in pre-peephole coords. Every
        // other compile passes no barrier and keeps byte-identical output.
        let seam_barrier_pre: Vec<usize> = match (global_slot_seed, user_main_entry) {
            (Some(_), Some(e)) => vec![e],
            _ => Vec::new(),
        };
        let (mut code, index_mapping) = profile::time("compile.peephole_pre_slotize", || {
            if seam_barrier_pre.is_empty() {
                stack_backend::optimize(code)
            } else {
                stack_backend::optimize_with_boundaries(code, &seam_barrier_pre)
            }
        });
        let source_map = apply_peephole_source_map(source_map, &index_mapping);

        // Update all function boundaries and entry point after optimization.
        // The index_mapping includes one extra entry for the end position.
        let entry =
            apply_peephole_index_mapping(&mut function_infos, entry, &index_mapping, &reused_base);
        // LV2: carry `user_main_entry` through the SAME mapping as `entry`.
        let user_main_entry = user_main_entry.map(|e| map_index_through(e, &index_mapping));

        let slotize_timer = profile::start("compile.slotize");
        for (idx, func_info) in function_infos.iter_mut().enumerate() {
            if reused_base[idx] {
                continue;
            }
            let code_start = func_info.code_start;
            let code_end = func_info.code_end;
            if code_start >= code_end || code_end > code.len() {
                continue;
            }
            // Abstract-numeric parameters (`x::Integer`/`x::Real`/... and
            // `T`-vars bounded by such an abstract type) map to a machine
            // `ValueType` (`I64`/`F64`) for dispatch, but their runtime value can
            // be a wider `BigInt`/`BigFloat`. Force their slots generic so the
            // slotizer can never emit a `LoadSlotI64`/`LoadSlotF64` that rejects
            // those values, independent of which inference path chose the load
            // instruction (Issue #9724).
            let generic_param_slots = abstract_numeric_param_slot_names(func_info);
            let slot_info = stack_backend::build_slot_info_with_generic_params(
                &func_info.params,
                &func_info.kwparams,
                &code[code_start..code_end],
                &generic_param_slots,
            );
            stack_backend::slotize_code(
                &mut code[code_start..code_end],
                &slot_info.name_to_slot,
                &slot_info.slot_types,
            );
            // Non-reused entries are unshared this run: make_mut is in-place.
            let func_info = std::rc::Rc::make_mut(func_info);
            func_info.slot_names = slot_info.slot_names;
            func_info.slot_types = slot_info.slot_types;
            func_info.local_slot_count = func_info.slot_names.len();
            func_info.param_slots = slot_info.param_slots;
            for (kw, slot) in func_info.kwparams.iter_mut().zip(slot_info.kwparam_slots) {
                kw.slot = slot;
            }
        }

        // LV2 (Issue #9199): when a live frame-0 layout was provided, SEED the
        // main block's global-slot assignment with it so every existing global
        // keeps its live slot index and a brand-new global appends after — the
        // slot alignment that lets the compiled delta main splice onto the live
        // VM. `code[entry..]` here is `[base_main | user_main]`; the base main's
        // stores land on their already-seeded base-global slots, so the seed's
        // ordering is preserved. Non-seeded compiles number global slots from 0
        // exactly as before.
        let global_slot_info = if entry < code.len() {
            let slot_info = match global_slot_seed {
                Some(seed) => stack_backend::build_global_slot_info_seeded(seed, &code[entry..]),
                None => stack_backend::build_slot_info(&[], &[], &code[entry..]),
            };
            stack_backend::slotize_code(
                &mut code[entry..],
                &slot_info.name_to_slot,
                &slot_info.slot_types,
            );
            slot_info
        } else {
            match global_slot_seed {
                Some(seed) => stack_backend::build_global_slot_info_seeded(seed, &[]),
                None => stack_backend::build_slot_info(&[], &[], &[]),
            }
        };
        // Slot indices, within the main/global block, of names still genuinely
        // bound at module/main scope once `compile_main` finished (Issue #9157).
        // `main_scope_names` excludes anything a `let`/`@testset` restored out of
        // scope on exit, even though such a name still occupies a slot here (it
        // shares the main frame's slot numbering) — see `MainStoreProtection`.
        // This only guards the specific store-elimination fusion below; the
        // REPL's own `Vm::get_global`-based persistence has no equivalent
        // scope filter, so a `let`-local can still leak into a later eval when
        // its store happens to survive for an unrelated reason (e.g. a
        // side-effecting statement between the assignment and its use defeats
        // that same fusion on its own) — tracked separately as Issue #9182.
        let protected_slots: HashSet<usize> = main_scope_names
            .iter()
            .filter_map(|name| global_slot_info.name_to_slot.get(name).copied())
            .collect();
        let global_slot_names = global_slot_info.slot_names;
        let global_slot_types = global_slot_info.slot_types;
        let global_slot_count = global_slot_names.len();
        profile::finish(slotize_timer);

        // Slotization can expose `LoadSlotI64; AddI64; StoreSlotI64` patterns that
        // the earlier name-based peephole pass cannot see. Run a second pass after
        // slot assignment so Issue #5091 superinstructions are emitted from the
        // final slotized bytecode.
        //
        // `code` at this point is function bodies followed by the main/global
        // block starting at `entry`; use the main-store-protecting variant so a
        // main-scope variable's store is not dropped just because its only use
        // is an immediate `Return*` within this compile unit — that store is
        // exactly what `Vm::get_global` reads back for REPL session persistence
        // across evaluations (Issue #9157). Function-body slots are unaffected
        // (their frames are discarded on return, so the standard elimination
        // still applies there), and so is a main-frame slot NOT in
        // `protected_slots` (e.g. a `let`-local that shares the frame's slot
        // numbering but was already scoped out by the end of compilation) —
        // that still gets the standard elimination too, so a `let` block cannot
        // leak a brand-new name into REPL persistence.
        // LV2 (Issue #9199): the same seam barrier for the post-slotize pass, in
        // post-first-pass coords (== current `user_main_entry`).
        let seam_barrier_post: Vec<usize> = match (global_slot_seed, user_main_entry) {
            (Some(_), Some(e)) => vec![e],
            _ => Vec::new(),
        };
        let (optimized_code, index_mapping) =
            profile::time("compile.peephole_post_slotize", || {
                let protection = subset_julia_vm_bytecode::peephole::MainStoreProtection {
                    main_entry: entry,
                    protected_slots: &protected_slots,
                };
                if seam_barrier_post.is_empty() {
                    stack_backend::optimize_protecting_main_stores(code, protection)
                } else {
                    stack_backend::optimize_protecting_main_stores_with_boundaries(
                        code,
                        protection,
                        &seam_barrier_post,
                    )
                }
            });
        let code = optimized_code;
        let source_map = apply_peephole_source_map(source_map, &index_mapping);
        let entry =
            apply_peephole_index_mapping(&mut function_infos, entry, &index_mapping, &reused_base);
        // LV2: carry `user_main_entry` through the post-slotize mapping too.
        let user_main_entry = user_main_entry.map(|e| map_index_through(e, &index_mapping));

        // LV2 (Issue #9199): the relocatable-delta compile (seed present) does NOT
        // assemble the O(session) Base/prior prefix into `code`. The REPL only
        // extracts `code[user_main_entry..]` (the isolated user main) to splice
        // onto the live VM — which ALREADY holds the prefix — so copying the whole
        // prefix each eval is exactly the O(session) compile cost the reshape must
        // remove. Skipping it leaves `code` = the freshly-compiled suffix (fresh
        // functions + base main + user main) with the user main at a suffix-relative
        // `user_main_entry`; the resulting `CompiledProgram` is NOT a runnable
        // standalone (missing prefix), but the live path never runs it — it slices
        // the user main out and discards the rest. Every ordinary compile still
        // assembles the prefix.
        let assemble_prefix = base_code_prefix.is_some() && global_slot_seed.is_none();
        let (mut code, source_map, entry) =
            profile::time("compile.cached_code_prefix_assemble", || {
                if let (true, Some(base_code_prefix)) = (assemble_prefix, base_code_prefix) {
                    let mut suffix = code;
                    let suffix_source_map = source_map;
                    relocate_jumps(&mut suffix, 0, base_code_prefix_len);
                    for (idx, func_info) in function_infos.iter_mut().enumerate() {
                        if reused_base.get(idx).copied().unwrap_or(false) {
                            continue;
                        }
                        // Non-reused entries are unshared this run: in-place.
                        let func_info = std::rc::Rc::make_mut(func_info);
                        func_info.entry += base_code_prefix_len;
                        func_info.code_start += base_code_prefix_len;
                        func_info.code_end += base_code_prefix_len;
                    }

                    let mut merged = Vec::with_capacity(base_code_prefix_len + suffix.len());
                    merged.extend_from_slice(base_code_prefix);
                    merged.extend(suffix);
                    let mut merged_source_map = vec![None; base_code_prefix_len];
                    merged_source_map.extend(suffix_source_map);
                    (merged, merged_source_map, entry + base_code_prefix_len)
                } else {
                    (code, source_map, entry)
                }
            });
        let mut source_map = source_map;
        // The prefix (when assembled) shifts the freshly-compiled suffix — which
        // holds the user main — forward by `base_code_prefix_len`, exactly as it
        // does `entry`; keep `user_main_entry` a valid offset into the FINAL `code`.
        // When the prefix is skipped (relocatable delta) the suffix stays at 0, so
        // no shift. Preload splices append AFTER this point, so a compile with
        // preload hits is rejected by the live path (its user main is not the tail).
        let user_main_entry = user_main_entry.map(|e| {
            if assemble_prefix {
                e + base_code_prefix_len
            } else {
                e
            }
        });

        // Issue #9189: splice each preload-cache-hit function's
        // already-finalized body in now, after both whole-buffer peephole
        // passes have run. That body is already slotized/peepholed (captured
        // verbatim from the original compile that produced the preload
        // cache), so it must not be re-processed by those passes the way a
        // freshly-compiled function's body is above — each one instead gets
        // its own relocate-and-append here, the same primitive
        // `compile_module_recursive`/`compile_main` use for their own
        // freshly-compiled chunks. Position varies per run (unlike the
        // always-position-0 cached-Base prefix above), so there is no flat
        // shift to apply — each function's `entry`/`code_start`/`code_end`
        // is set directly from where it lands in `code` here.
        // Issue #9230: how many preloaded bodies were actually spliced this
        // compile — 0 when the cache was absent OR the `closure_layout` gate
        // deactivated it (a program whose non-Base prefix didn't match). Lets
        // callers/tests confirm the whole-prefix reuse actually fired rather
        // than silently falling back to a normal (still-correct) compile.
        let preload_spliced_count = preload_reused.len();
        for (func_info_idx, cached) in preload_reused {
            let mut body = cached.body.clone();
            let entry_pos = code.len();
            relocate_jumps(&mut body, 0, entry_pos);
            source_map.extend(std::iter::repeat_n(None, body.len()));
            code.extend(body);

            let fi = std::rc::Rc::make_mut(&mut function_infos[func_info_idx]);
            fi.entry = entry_pos;
            fi.code_start = entry_pos;
            fi.code_end = code.len();
        }

        // Issue #8555: cached Base bytecode carries compile-time-frozen
        // candidate lists at its named dynamic-dispatch sites; refresh them
        // with the user program's hook methods so extending e.g.
        // `promote_rule` no longer requires bypassing the Base cache.
        // Skipped for the relocatable delta (`!assemble_prefix`): the Base prefix is
        // not in `code`, and a delta adds no methods, so there is nothing to refresh
        // (Issue #9199 LV2).
        if assemble_prefix && base_code_prefix_len > 0 {
            profile::time("compile.refresh_cached_base_dispatch_candidates", || {
                refresh_cached_base_dispatch_candidates(
                    &mut code[..base_code_prefix_len],
                    &method_tables,
                    &function_infos,
                    base_function_count,
                );
            });
        }

        // Lazy AoT: Build RuntimeCompileContext for specialization
        let final_assembly_timer = profile::start("compile.final_assembly");
        // Issue #6657: detect a user `getindex` override on a native array-like
        // receiver so the runtime specializer skips its native-indexing fast path
        // for scalar `xs[i]` (which would bypass the override). User-origin is
        // `global_index >= base_function_count`; Base array `getindex` methods are
        // excluded so the common no-override program is unaffected.
        let disable_array_getindex_specialization =
            ["getindex", "Base.getindex"].iter().any(|name| {
                method_tables.get(*name).is_some_and(|table| {
                    table.methods.iter().any(|m| {
                        !m.is_base_program_method(base_function_count)
                            && m.param_matches_at(0, method_table::core_type_is_array_like)
                    })
                })
            });
        // Issue #6806: the same detection for a user `setindex!` override on a
        // native array-like receiver (param 0) so the IndexStore write fast path
        // is refused, reaching the override via dispatch.
        let disable_array_setindex_specialization =
            ["setindex!", "Base.setindex!"].iter().any(|name| {
                method_tables.get(*name).is_some_and(|table| {
                    table.methods.iter().any(|m| {
                        !m.is_base_program_method(base_function_count)
                            && m.param_matches_at(0, method_table::core_type_is_array_like)
                    })
                })
            });
        // Issue #8127: detect any user `getproperty` override so the function
        // specializer refuses its direct-`GetField` fast path for `obj.field`
        // reads (which would bypass the override). User-origin is `global_index
        // >= base_function_count`; the Base default `getproperty(x, ::Symbol)` is
        // excluded so the common no-override program keeps the field fast path.
        let disable_field_access_specialization =
            ["getproperty", "Base.getproperty"].iter().any(|name| {
                method_tables.get(*name).is_some_and(|table| {
                    table
                        .methods
                        .iter()
                        .any(|m| !m.is_base_program_method(base_function_count))
                })
            });
        let specialization_disable_flags = SpecializationDisableFlags {
            array_getindex: disable_array_getindex_specialization,
            array_setindex: disable_array_setindex_specialization,
            field_access: disable_field_access_specialization,
        };
        let mut module_base_exports_visibility = HashMap::new();
        let mut module_implicit_standard_bindings = HashMap::new();
        for module in &all_modules {
            collect_module_base_exports_visibility(module, "", &mut module_base_exports_visibility);
            collect_module_implicit_standard_bindings(
                module,
                "",
                &mut module_implicit_standard_bindings,
            );
        }
        let base_exported_names: HashSet<String> = crate::julia::base::exported_names()
            .iter()
            .cloned()
            .collect();
        // NB: `disable_field_access_specialization` is intentionally NOT a
        // context-activation trigger — a getproperty override alone must not
        // newly enable specialization. When no other trigger fires, the context
        // stays `None`, no function is specialized, and the interpreter's
        // `getproperty` routing (compile/expr/struct_.rs) already reaches the
        // override. The flag only matters once specialization is otherwise active.
        let compile_context = if !specializable_functions.is_empty()
            || !shared_ctx.parametric_structs.is_empty()
            || !shared_ctx.base_parametric_structs.is_empty()
            || !shared_ctx.type_aliases.is_empty()
            || !shared_ctx.module_imported_bindings.is_empty()
            || !primitive_types.is_empty()
            || !module_base_exports_visibility.is_empty()
        {
            Some(RuntimeCompileContext {
                struct_table: shared_ctx.struct_table.clone(),
                struct_defs: shared_ctx.struct_defs.clone(),
                parametric_structs: shared_ctx.parametric_structs.clone(),
                base_parametric_structs: shared_ctx.base_parametric_structs.clone(),
                type_aliases: shared_ctx.type_aliases.clone(),
                module_imported_bindings: shared_ctx.module_imported_bindings.clone(),
                module_base_exports_visibility,
                module_implicit_standard_bindings,
                base_exported_names,
                inference_global_types: shared_ctx.inference_global_types.clone(),
                primitive_types: primitive_types.clone(),
                disable_array_getindex_specialization: specialization_disable_flags.array_getindex,
                disable_array_setindex_specialization: specialization_disable_flags.array_setindex,
                disable_field_access_specialization: specialization_disable_flags.field_access,
                module_registry: module_registry.clone(),
            })
        } else {
            None
        };
        let mut runtime_specialization_map: Vec<(usize, usize)> = shared_ctx
            .spec_func_mapping
            .iter()
            .map(|(&fallback_index, &spec_index)| (fallback_index, spec_index))
            .collect();
        runtime_specialization_map
            .sort_unstable_by_key(|&(fallback_index, spec_index)| (spec_index, fallback_index));
        let mut inference_global_types_snapshot: Vec<(String, ValueType)> = compile_context
            .as_ref()
            .map(|context| {
                context
                    .inference_global_types
                    .iter()
                    .map(|(name, ty)| (name.clone(), ty.clone()))
                    .collect()
            })
            .unwrap_or_default();
        inference_global_types_snapshot.sort_unstable_by(|left, right| left.0.cmp(&right.0));

        // Macro binding table for function-form `isdefined(::Module, Symbol("@m"))`
        // reflection (Issue #7948). Macros are expanded away during lowering, so the
        // VM has no macro registry at runtime; record per-module which macro names
        // are visible so the reflection path can answer correctly. Keyed by
        // `ModuleId` (Issue #10988 Phase 2a; was a bare module-path `String`) —
        // `module_registry` already assigned every real module path an id in
        // deterministic registration order (`collect_module_metadata`); `.intern`
        // (idempotent) rather than `.lookup` so a REPL relocatable-delta path's
        // metadata carried in from `extra_module_metadata` (whose module paths
        // are not walked by `register_module_ids` — Issue #10988 known
        // limitation, see docs/vm/SEMANTIC_ID_MIGRATION.md) still gets a valid,
        // collision-free id instead of silently dropping the entry.
        let mut macro_bindings: HashMap<ModuleId, HashSet<String>> = HashMap::new();
        // Module-qualified surface: `isdefined(AbstractAlgebra, Symbol("@alias"))`.
        // `module_functions` already carries each module's `@name` macro entries.
        for (module_path, names) in &module_functions {
            let macros: HashSet<String> = names
                .iter()
                .filter(|n| n.starts_with('@'))
                .cloned()
                .collect();
            if !macros.is_empty() {
                let module_id = module_registry.intern(module_path);
                macro_bindings.entry(module_id).or_default().extend(macros);
            }
        }
        // Main-visible surface: `isdefined(Main, Symbol("@alias"))`. Top-level
        // (Main-owned) macros plus macros pulled in by `using` (already collected,
        // export-respecting, in `imported_functions`).
        {
            let main_id = module_registry.intern("Main");
            let main_macros = macro_bindings.entry(main_id).or_default();
            for m in &program.macros {
                main_macros.insert(format!("@{}", m.name));
            }
            for name in &imported_functions {
                if name.starts_with('@') {
                    main_macros.insert(name.clone());
                }
            }
            if main_macros.is_empty() {
                macro_bindings.remove(&main_id);
            }
        }

        let mut enum_defs = if global_slot_seed.is_some() {
            precompiled_base
                .map(|prefix| prefix.enum_defs.clone())
                .unwrap_or_default()
        } else {
            Vec::new()
        };
        collect_enum_def_infos(&program.main, &mut enum_defs);

        let compiled = CompiledProgram {
            code,
            source_map,
            functions: function_infos,
            struct_defs: shared_ctx.struct_defs,
            abstract_types,
            primitive_types,
            enum_defs,
            show_methods,
            print_methods,
            entry,
            specializable_functions,
            runtime_specialization_map,
            inference_global_types_snapshot,
            specialization_disable_flags,
            compile_context,
            base_function_count,
            macro_bindings,
            module_registry,
            global_slot_names,
            global_slot_types,
            global_slot_count,
            main_scope_names,
        };

        let inference_results = inference_engine.snapshot_return_cache();
        profile::finish(final_assembly_timer);

        Ok(CoreCompileOutput {
            compiled,
            method_tables,
            closure_captures: shared_ctx.closure_captures,
            inference_results,
            source_ordered_method_sigs: shared_ctx.source_ordered_method_sigs,
            module_function_infos,
            nonbase_layout,
            preload_spliced_count,
            cached_base_extra_reused_count,
            user_main_entry,
        })
    }
}

/// Phase 1: use `base_function_count` from the program if Base was already
/// merged by lib.rs, otherwise merge with the precompiled Base prelude now
/// (for JSON IR input that doesn't use the lib.rs pipeline).
fn merge_precompiled_base(program: &Program) -> (std::borrow::Cow<'_, Program>, usize) {
    profile::time("compile.merge_precompiled_base", || {
        if program.base_function_count > 0 {
            // Already merged by lib.rs - use as-is
            (
                std::borrow::Cow::Borrowed(program),
                program.base_function_count,
            )
        } else {
            // Not merged yet (e.g., JSON IR) - merge now
            let merged = merge_with_precompiled_base(program);
            (
                std::borrow::Cow::Owned(merged.program),
                merged.base_function_count,
            )
        }
    })
}

/// Phase 2: inline small pure user functions into the IR, then run the
/// pure-expression optimization pass over the user segment only.
fn inline_and_optimize_ir(
    program: &Program,
    base_function_count: usize,
    has_opaque_runtime_eval: bool,
    current_source_function_count: Option<usize>,
) -> (std::borrow::Cow<'_, Program>, ir_opt::UserSegmentOptimized) {
    if has_opaque_runtime_eval {
        // Opaque runtime code evaluation can define or redefine methods whose
        // names are not visible to the compiler. Keep user IR un-inlined so
        // later bytecode emission can route affected global calls through
        // runtime dispatch instead of baking stale callee bodies into callers
        // (Issue #8825), while preserving the user-segment optimizer that
        // existing metaprogramming fixtures rely on for lowered helper calls.
        let mut optimized_user_segment =
            ir_opt::optimize_pure_expressions_user_only(program, base_function_count);
        // Issue #9198 S2/S3: 2-field-f64 isbits-struct slot-pair SROA on the user
        // segment (Complex{Float64} + user 2-field f64 structs).
        complex_sroa::apply_to_user_segment(&mut optimized_user_segment, &program.structs);
        return (std::borrow::Cow::Borrowed(program), optimized_user_segment);
    }

    let inlined_program = profile::time("compile.ir_inline", || {
        ir_inline::inline_small_pure_functions_cow(
            program,
            base_function_count,
            current_source_function_count,
        )
    });
    let mut optimized_user_segment = profile::time("compile.ir_opt", || {
        ir_opt::optimize_pure_expressions_user_only(inlined_program.as_ref(), base_function_count)
    });
    // Issue #9198 S2/S3: 2-field-f64 isbits-struct slot-pair SROA on the user
    // segment — unbox proven Complex{Float64} (and user 2-field f64 struct) loop
    // locals into `f64` re/im slot pairs so the typed `z = z*z + c` (or
    // `p = V2(p.x+1, p.y+2)`) loop issues zero per-iteration heap allocations.
    // Runs after ir_opt so it operates on the settled user IR; the resulting f64
    // ops are fused by the later bytecode peephole (see `complex_sroa`).
    profile::time("compile.complex_sroa", || {
        complex_sroa::apply_to_user_segment(&mut optimized_user_segment, &inlined_program.structs)
    });
    (inlined_program, optimized_user_segment)
}

struct OpaqueRuntimeEvalScan {
    has_opaque_runtime_eval: bool,
    has_runtime_nominal: bool,
    runtime_nominal_names: HashSet<String>,
    function_names: HashSet<String>,
}

/// True when `program` contains opaque runtime code evaluation (`eval`,
/// `@eval`, `include`, `include_string`, generated functions) that can define
/// or redefine methods the compiler cannot see. The REPL input-delta path
/// (Issue #9199 S5) routes such inputs through the full recompile path, since a
/// runtime-defined method would not be in the reused precompiled prefix.
pub(crate) fn program_defines_via_opaque_eval(program: &Program) -> bool {
    user_segment_opaque_runtime_eval(program, 0).has_opaque_runtime_eval
}

/// Whether the program's MAIN block contains a hard-scope block — a `let`
/// (`Expr::LetBlock`) or `@testset` (`Stmt::TestSet`) — ANYWHERE, including nested
/// in a loop/branch body or an expression (Issue #9199 LV2). Such a block can bind
/// a local that SHADOWS a live global; the compiler compiles the shadow to that
/// global's frame-0 slot and emits `ForgetLetLocals` at block exit. The delta path
/// (which has no compile-time outer binding for the value-carried global) would
/// use that to CLEAR the live global rather than a transient local, silently
/// dropping the binding. The REPL therefore routes any input containing a
/// hard-scope block through the full recompile path, which establishes the global
/// as an outer binding and RESTORES (never forgets) the shadow. Conservative: it
/// rejects even a non-shadowing `let`, whose full-path handling is identical.
pub(crate) fn program_main_has_hard_scope_block(program: &Program) -> bool {
    block_has_hard_scope(&program.main)
}

fn block_has_hard_scope(block: &crate::ir::core::Block) -> bool {
    block.stmts.iter().any(stmt_has_hard_scope)
}

fn stmt_has_hard_scope(stmt: &Stmt) -> bool {
    use crate::compile::inference::contains_letblock;
    match stmt {
        // `@testset` is itself a hard scope with its own local frame.
        Stmt::TestSet { .. } => true,
        Stmt::Block(b) | Stmt::Timed { body: b, .. } => block_has_hard_scope(b),
        Stmt::For {
            start, end, body, ..
        } => contains_letblock(start) || contains_letblock(end) || block_has_hard_scope(body),
        Stmt::ForEach { iterable, body, .. } | Stmt::ForEachTuple { iterable, body, .. } => {
            contains_letblock(iterable) || block_has_hard_scope(body)
        }
        Stmt::While {
            condition, body, ..
        } => contains_letblock(condition) || block_has_hard_scope(body),
        Stmt::If {
            condition,
            then_branch,
            else_branch,
            ..
        } => {
            contains_letblock(condition)
                || block_has_hard_scope(then_branch)
                || else_branch.as_ref().is_some_and(block_has_hard_scope)
        }
        Stmt::Try {
            try_block,
            catch_block,
            else_block,
            finally_block,
            ..
        } => {
            block_has_hard_scope(try_block)
                || catch_block.as_ref().is_some_and(block_has_hard_scope)
                || else_block.as_ref().is_some_and(block_has_hard_scope)
                || finally_block.as_ref().is_some_and(block_has_hard_scope)
        }
        Stmt::Expr { expr, .. } => contains_letblock(expr),
        // A hard-scope `let` in the INDEX / KEY expression is just as dangerous as
        // one in the RHS (Issue #9199 review r3536211269): `a[let x = 99; 1 end] = v`
        // / `d[let x = 99; :k end] = v` bind a local that can shadow a live global,
        // and the fresh-delta path would emit `ForgetLetLocals(["x"])` that clears
        // frame 0 — corrupting the VM that then gets parked for the next live delta.
        // Scan the index/key exprs too, not only `value`.
        Stmt::IndexAssign { indices, value, .. } => {
            indices.iter().any(contains_letblock) || contains_letblock(value)
        }
        Stmt::DictAssign { key, value, .. } => contains_letblock(key) || contains_letblock(value),
        Stmt::Assign { value, .. }
        | Stmt::AddAssign { value, .. }
        | Stmt::FieldAssign { value, .. }
        | Stmt::DestructuringAssign { value, .. } => contains_letblock(value),
        Stmt::Return { value, .. } => value.as_ref().is_some_and(contains_letblock),
        Stmt::Test { condition, .. } => contains_letblock(condition),
        Stmt::TestThrows { expr, .. } => contains_letblock(expr),
        // Break/Continue/Meta/Using/Export/FunctionDef/EvalFunctionDef/Label/Goto/
        // EnumDef/Global: no expression that can host a top-level hard-scope `let`
        // reaching module globals. (A `let` inside a lifted lambda body is handled
        // by the function-lift rejection, not here.)
        _ => false,
    }
}

fn user_segment_opaque_runtime_eval(
    program: &Program,
    base_function_count: usize,
) -> OpaqueRuntimeEvalScan {
    let base_function_count = base_function_count.min(program.functions.len());
    let mut scan = OpaqueRuntimeEvalScan {
        has_opaque_runtime_eval: false,
        has_runtime_nominal: false,
        runtime_nominal_names: HashSet::new(),
        function_names: HashSet::new(),
    };
    for function in &program.functions[base_function_count..] {
        scan_function_opaque_runtime_eval(function, &mut scan);
    }
    for module in &program.modules {
        scan_module_opaque_runtime_eval(module, &mut scan);
    }
    scan_block_opaque_runtime_eval(&program.main, &mut scan);
    scan
}

fn scan_module_opaque_runtime_eval(
    module: &crate::ir::core::Module,
    scan: &mut OpaqueRuntimeEvalScan,
) {
    for function in &module.functions {
        scan_function_opaque_runtime_eval(function, scan);
    }
    scan_block_opaque_runtime_eval(&module.body, scan);
    for submodule in &module.submodules {
        scan_module_opaque_runtime_eval(submodule, scan);
    }
}

fn scan_function_opaque_runtime_eval(func: &Function, scan: &mut OpaqueRuntimeEvalScan) {
    for kw in &func.kwparams {
        scan_expr_opaque_runtime_eval(&kw.default, scan);
    }
    scan_block_opaque_runtime_eval(&func.body, scan);
}

fn scan_block_opaque_runtime_eval(block: &Block, scan: &mut OpaqueRuntimeEvalScan) {
    for stmt in &block.stmts {
        scan_stmt_opaque_runtime_eval(stmt, scan);
    }
}

pub(in crate::compile) fn collect_runtime_nominal_names_in_block(block: &Block) -> HashSet<String> {
    collect_runtime_nominal_names_in_statements(&block.stmts)
}

fn collect_runtime_nominal_names_in_statements(stmts: &[Stmt]) -> HashSet<String> {
    let mut scan = OpaqueRuntimeEvalScan {
        has_opaque_runtime_eval: false,
        has_runtime_nominal: false,
        runtime_nominal_names: HashSet::new(),
        function_names: HashSet::new(),
    };
    for stmt in stmts {
        scan_stmt_opaque_runtime_eval(stmt, &mut scan);
    }
    scan.runtime_nominal_names
}

fn collect_module_runtime_inner_constructor_structs<'a>(
    module: &'a crate::ir::core::Module,
    prefix: &str,
    output: &mut Vec<(&'a crate::ir::core::StructDef, Option<String>)>,
) {
    let module_path = if prefix.is_empty() {
        module.name.clone()
    } else {
        format!("{prefix}.{}", module.name)
    };
    collect_runtime_inner_constructor_structs_in_block(
        &module.body,
        Some(module_path.clone()),
        output,
    );
    for submodule in &module.submodules {
        collect_module_runtime_inner_constructor_structs(submodule, &module_path, output);
    }
}

fn collect_runtime_inner_constructor_structs_in_block<'a>(
    block: &'a Block,
    module_path: Option<String>,
    output: &mut Vec<(&'a crate::ir::core::StructDef, Option<String>)>,
) {
    for statement in &block.stmts {
        collect_runtime_inner_constructor_structs_in_stmt(statement, module_path.clone(), output);
    }
}

fn collect_runtime_inner_constructor_structs_in_stmt<'a>(
    statement: &'a Stmt,
    module_path: Option<String>,
    output: &mut Vec<(&'a crate::ir::core::StructDef, Option<String>)>,
) {
    let mut visit_block = |block: &'a Block| {
        collect_runtime_inner_constructor_structs_in_block(block, module_path.clone(), output)
    };
    match statement {
        Stmt::Block(block)
        | Stmt::Timed { body: block, .. }
        | Stmt::TestSet { body: block, .. } => visit_block(block),
        Stmt::For { body, .. }
        | Stmt::ForEach { body, .. }
        | Stmt::ForEachTuple { body, .. }
        | Stmt::While { body, .. } => visit_block(body),
        Stmt::If {
            condition,
            then_branch,
            else_branch,
            ..
        } => match super::stmt::const_bool_condition_with_lookup(condition, &|_| None) {
            Some(true) => visit_block(then_branch),
            Some(false) => {
                if let Some(else_branch) = else_branch {
                    visit_block(else_branch);
                }
            }
            None => {
                visit_block(then_branch);
                if let Some(else_branch) = else_branch {
                    visit_block(else_branch);
                }
            }
        },
        Stmt::Try {
            try_block,
            catch_block,
            else_block,
            finally_block,
            ..
        } => {
            visit_block(try_block);
            for block in [catch_block, else_block, finally_block]
                .into_iter()
                .flatten()
            {
                visit_block(block);
            }
        }
        Stmt::Assign { value, .. }
        | Stmt::AddAssign { value, .. }
        | Stmt::Return {
            value: Some(value), ..
        }
        | Stmt::Expr { expr: value, .. }
        | Stmt::Test {
            condition: value, ..
        }
        | Stmt::FieldAssign { value, .. }
        | Stmt::DestructuringAssign { value, .. } => {
            collect_runtime_inner_constructor_structs_in_expr(value, module_path, output);
        }
        Stmt::IndexAssign { indices, value, .. } => {
            for index in indices {
                collect_runtime_inner_constructor_structs_in_expr(
                    index,
                    module_path.clone(),
                    output,
                );
            }
            collect_runtime_inner_constructor_structs_in_expr(value, module_path, output);
        }
        Stmt::DictAssign { key, value, .. } => {
            collect_runtime_inner_constructor_structs_in_expr(key, module_path.clone(), output);
            collect_runtime_inner_constructor_structs_in_expr(value, module_path, output);
        }
        Stmt::TestThrows { expr, .. } => {
            collect_runtime_inner_constructor_structs_in_expr(expr, module_path, output)
        }
        Stmt::RuntimeNominalDef {
            definition: RuntimeNominalDef::Struct(definition),
            ..
        } if definition.type_params.is_empty() && !definition.inner_constructors.is_empty() => {
            output.push((definition, module_path));
        }
        Stmt::RuntimeNominalDef { .. }
        | Stmt::FunctionDef { .. }
        | Stmt::EvalFunctionDef { .. }
        | Stmt::Break { .. }
        | Stmt::Continue { .. }
        | Stmt::Meta { .. }
        | Stmt::Using { .. }
        | Stmt::Export { .. }
        | Stmt::Label { .. }
        | Stmt::Goto { .. }
        | Stmt::EnumDef { .. }
        | Stmt::Global { .. }
        | Stmt::LocalDecl { .. }
        | Stmt::Return { value: None, .. } => {}
    }
}

fn collect_runtime_inner_constructor_structs_in_expr<'a>(
    expression: &'a Expr,
    module_path: Option<String>,
    output: &mut Vec<(&'a crate::ir::core::StructDef, Option<String>)>,
) {
    let mut visit = |expression: &'a Expr| {
        collect_runtime_inner_constructor_structs_in_expr(expression, module_path.clone(), output)
    };
    match expression {
        Expr::BinaryOp { left, right, .. }
        | Expr::Pair {
            key: left,
            value: right,
            ..
        } => {
            visit(left);
            visit(right);
        }
        Expr::UnaryOp { operand, .. }
        | Expr::Convert { operand, .. }
        | Expr::QuoteLiteral {
            constructor: operand,
            ..
        }
        | Expr::AssignExpr { value: operand, .. } => visit(operand),
        Expr::Call { args, kwargs, .. } | Expr::ModuleCall { args, kwargs, .. } => {
            for argument in args {
                visit(argument);
            }
            for (_, value) in kwargs {
                visit(value);
            }
        }
        Expr::Builtin { args, .. }
        | Expr::ArrayLiteral { elements: args, .. }
        | Expr::TupleLiteral { elements: args, .. }
        | Expr::StringConcat { parts: args, .. }
        | Expr::New { args, .. } => {
            for argument in args {
                visit(argument);
            }
        }
        Expr::Index { array, indices, .. } => {
            visit(array);
            for index in indices {
                visit(index);
            }
        }
        Expr::Range {
            start, step, stop, ..
        } => {
            visit(start);
            if let Some(step) = step {
                visit(step);
            }
            visit(stop);
        }
        Expr::Comprehension {
            body, iter, filter, ..
        }
        | Expr::Generator {
            body, iter, filter, ..
        } => {
            visit(body);
            visit(iter);
            if let Some(filter) = filter {
                visit(filter);
            }
        }
        Expr::MultiComprehension {
            body,
            iterations,
            filter,
            ..
        } => {
            visit(body);
            for (_, iteration) in iterations {
                visit(iteration);
            }
            if let Some(filter) = filter {
                visit(filter);
            }
        }
        Expr::FieldAccess { object, .. } => visit(object),
        Expr::NamedTupleLiteral { fields, .. } => {
            for (_, value) in fields {
                visit(value);
            }
        }
        Expr::DictLiteral { pairs, .. } => {
            for (key, value) in pairs {
                visit(key);
                visit(value);
            }
        }
        Expr::LetBlock { bindings, body, .. } => {
            for (_, value) in bindings {
                visit(value);
            }
            collect_runtime_inner_constructor_structs_in_block(body, module_path, output);
        }
        Expr::Ternary {
            condition,
            then_expr,
            else_expr,
            ..
        } => {
            visit(condition);
            visit(then_expr);
            visit(else_expr);
        }
        Expr::DynamicTypeConstruct {
            base_expr,
            type_args,
            ..
        } => {
            if let Some(base_expr) = base_expr {
                visit(base_expr);
            }
            for type_arg in type_args {
                visit(type_arg);
            }
        }
        Expr::ReturnExpr {
            value: Some(value), ..
        } => visit(value),
        Expr::Literal(_, _)
        | Expr::Var(_, _)
        | Expr::TypedEmptyArray { .. }
        | Expr::SliceAll { .. }
        | Expr::FunctionRef { .. }
        | Expr::ReturnExpr { value: None, .. }
        | Expr::BreakExpr { .. }
        | Expr::ContinueExpr { .. } => {}
    }
}

fn scan_stmt_opaque_runtime_eval(stmt: &Stmt, scan: &mut OpaqueRuntimeEvalScan) {
    match stmt {
        Stmt::Block(block)
        | Stmt::Timed { body: block, .. }
        | Stmt::TestSet { body: block, .. } => scan_block_opaque_runtime_eval(block, scan),
        Stmt::Assign { value, .. }
        | Stmt::AddAssign { value, .. }
        | Stmt::Return {
            value: Some(value), ..
        }
        | Stmt::Expr { expr: value, .. }
        | Stmt::Test {
            condition: value, ..
        }
        | Stmt::IndexAssign { value, .. }
        | Stmt::FieldAssign { value, .. }
        | Stmt::DestructuringAssign { value, .. } => scan_expr_opaque_runtime_eval(value, scan),
        Stmt::For {
            start,
            end,
            step,
            body,
            ..
        } => {
            scan_expr_opaque_runtime_eval(start, scan);
            scan_expr_opaque_runtime_eval(end, scan);
            if let Some(step) = step {
                scan_expr_opaque_runtime_eval(step, scan);
            }
            scan_block_opaque_runtime_eval(body, scan);
        }
        Stmt::ForEach { iterable, body, .. } | Stmt::ForEachTuple { iterable, body, .. } => {
            scan_expr_opaque_runtime_eval(iterable, scan);
            scan_block_opaque_runtime_eval(body, scan);
        }
        Stmt::While {
            condition, body, ..
        } => {
            scan_expr_opaque_runtime_eval(condition, scan);
            scan_block_opaque_runtime_eval(body, scan);
        }
        Stmt::If {
            condition,
            then_branch,
            else_branch,
            ..
        } => {
            scan_expr_opaque_runtime_eval(condition, scan);
            scan_block_opaque_runtime_eval(then_branch, scan);
            if let Some(else_branch) = else_branch {
                scan_block_opaque_runtime_eval(else_branch, scan);
            }
        }
        Stmt::Try {
            try_block,
            catch_block,
            else_block,
            finally_block,
            ..
        } => {
            scan_block_opaque_runtime_eval(try_block, scan);
            if let Some(catch_block) = catch_block {
                scan_block_opaque_runtime_eval(catch_block, scan);
            }
            if let Some(else_block) = else_block {
                scan_block_opaque_runtime_eval(else_block, scan);
            }
            if let Some(finally_block) = finally_block {
                scan_block_opaque_runtime_eval(finally_block, scan);
            }
        }
        Stmt::TestThrows { expr, .. } => scan_expr_opaque_runtime_eval(expr, scan),
        Stmt::DictAssign { key, value, .. } => {
            scan_expr_opaque_runtime_eval(key, scan);
            scan_expr_opaque_runtime_eval(value, scan);
        }
        Stmt::FunctionDef { func, .. } => scan_function_opaque_runtime_eval(func, scan),
        Stmt::EvalFunctionDef { .. }
        | Stmt::Break { .. }
        | Stmt::Continue { .. }
        | Stmt::Meta { .. }
        | Stmt::LocalDecl { .. }
        | Stmt::Using { .. }
        | Stmt::Export { .. }
        | Stmt::Label { .. }
        | Stmt::Goto { .. }
        | Stmt::EnumDef { .. }
        | Stmt::Global { .. }
        | Stmt::Return { value: None, .. } => {}
        Stmt::RuntimeNominalDef { definition, .. } => {
            scan.has_runtime_nominal = true;
            let name = match definition {
                RuntimeNominalDef::Struct(definition) => &definition.name,
                RuntimeNominalDef::AbstractType(definition) => &definition.name,
                RuntimeNominalDef::PrimitiveType(definition) => &definition.name,
                RuntimeNominalDef::Enum(definition) => &definition.name,
            };
            scan.runtime_nominal_names.insert(name.clone());
        }
    }
}

fn scan_expr_opaque_runtime_eval(expr: &Expr, scan: &mut OpaqueRuntimeEvalScan) {
    match expr {
        Expr::Call {
            function,
            args,
            kwargs,
            ..
        } => {
            if is_opaque_runtime_eval_name(function) {
                scan.has_opaque_runtime_eval = true;
                collect_eval_defined_method_names(args, scan);
            }
            for arg in args {
                scan_expr_opaque_runtime_eval(arg, scan);
            }
            for (_, value) in kwargs {
                scan_expr_opaque_runtime_eval(value, scan);
            }
        }
        Expr::ModuleCall {
            function,
            args,
            kwargs,
            ..
        } => {
            if is_opaque_runtime_eval_name(function) {
                scan.has_opaque_runtime_eval = true;
                collect_eval_defined_method_names(args, scan);
            }
            for arg in args {
                scan_expr_opaque_runtime_eval(arg, scan);
            }
            for (_, value) in kwargs {
                scan_expr_opaque_runtime_eval(value, scan);
            }
        }
        Expr::Builtin { name, args, .. } => {
            if matches!(
                name,
                BuiltinOp::Eval
                    | BuiltinOp::GeneratedEval
                    | BuiltinOp::IncludeString
                    | BuiltinOp::EvalFile
            ) {
                scan.has_opaque_runtime_eval = true;
                if matches!(name, BuiltinOp::Eval | BuiltinOp::GeneratedEval) {
                    collect_eval_defined_method_names(args, scan);
                }
            }
            for arg in args {
                scan_expr_opaque_runtime_eval(arg, scan);
            }
        }
        Expr::BinaryOp { left, right, .. }
        | Expr::Pair {
            key: left,
            value: right,
            ..
        } => {
            scan_expr_opaque_runtime_eval(left, scan);
            scan_expr_opaque_runtime_eval(right, scan);
        }
        Expr::UnaryOp { operand, .. }
        | Expr::Convert { operand, .. }
        | Expr::QuoteLiteral {
            constructor: operand,
            ..
        }
        | Expr::AssignExpr { value: operand, .. } => scan_expr_opaque_runtime_eval(operand, scan),
        Expr::ReturnExpr {
            value: Some(value), ..
        } => scan_expr_opaque_runtime_eval(value, scan),
        Expr::ArrayLiteral { elements, .. } | Expr::TupleLiteral { elements, .. } => {
            for element in elements {
                scan_expr_opaque_runtime_eval(element, scan);
            }
        }
        Expr::New { args, .. } => {
            for arg in args {
                scan_expr_opaque_runtime_eval(arg, scan);
            }
        }
        Expr::Index { array, indices, .. } => {
            scan_expr_opaque_runtime_eval(array, scan);
            for index in indices {
                scan_expr_opaque_runtime_eval(index, scan);
            }
        }
        Expr::Range {
            start, step, stop, ..
        } => {
            scan_expr_opaque_runtime_eval(start, scan);
            if let Some(step) = step {
                scan_expr_opaque_runtime_eval(step, scan);
            }
            scan_expr_opaque_runtime_eval(stop, scan);
        }
        Expr::Comprehension {
            body, iter, filter, ..
        }
        | Expr::Generator {
            body, iter, filter, ..
        } => {
            scan_expr_opaque_runtime_eval(body, scan);
            scan_expr_opaque_runtime_eval(iter, scan);
            if let Some(filter) = filter {
                scan_expr_opaque_runtime_eval(filter, scan);
            }
        }
        Expr::MultiComprehension {
            body,
            iterations,
            filter,
            ..
        } => {
            scan_expr_opaque_runtime_eval(body, scan);
            for (_, iter) in iterations {
                scan_expr_opaque_runtime_eval(iter, scan);
            }
            if let Some(filter) = filter {
                scan_expr_opaque_runtime_eval(filter, scan);
            }
        }
        Expr::FieldAccess { object, .. } => scan_expr_opaque_runtime_eval(object, scan),
        Expr::NamedTupleLiteral { fields, .. } => {
            for (_, value) in fields {
                scan_expr_opaque_runtime_eval(value, scan);
            }
        }
        Expr::DictLiteral { pairs, .. } => {
            for (key, value) in pairs {
                scan_expr_opaque_runtime_eval(key, scan);
                scan_expr_opaque_runtime_eval(value, scan);
            }
        }
        Expr::LetBlock { bindings, body, .. } => {
            for (_, value) in bindings {
                scan_expr_opaque_runtime_eval(value, scan);
            }
            scan_block_opaque_runtime_eval(body, scan);
        }
        Expr::StringConcat { parts, .. } => {
            for part in parts {
                scan_expr_opaque_runtime_eval(part, scan);
            }
        }
        Expr::Ternary {
            condition,
            then_expr,
            else_expr,
            ..
        } => {
            scan_expr_opaque_runtime_eval(condition, scan);
            scan_expr_opaque_runtime_eval(then_expr, scan);
            scan_expr_opaque_runtime_eval(else_expr, scan);
        }
        Expr::DynamicTypeConstruct {
            base_expr,
            type_args,
            ..
        } => {
            if let Some(base_expr) = base_expr {
                scan_expr_opaque_runtime_eval(base_expr, scan);
            }
            for type_arg in type_args {
                scan_expr_opaque_runtime_eval(type_arg, scan);
            }
        }
        Expr::Literal(_, _)
        | Expr::Var(_, _)
        | Expr::TypedEmptyArray { .. }
        | Expr::SliceAll { .. }
        | Expr::FunctionRef { .. }
        | Expr::ReturnExpr { value: None, .. }
        | Expr::BreakExpr { .. }
        | Expr::ContinueExpr { .. } => {}
    }
}

fn collect_eval_defined_method_names(args: &[Expr], scan: &mut OpaqueRuntimeEvalScan) {
    for arg in args {
        if let Some(name) = eval_defined_method_name(arg) {
            scan.function_names.insert(name);
        }
    }
}

fn eval_defined_method_name(expr: &Expr) -> Option<String> {
    match expr {
        Expr::QuoteLiteral { constructor, .. } => method_name_from_quote_constructor(constructor),
        Expr::Literal(literal, _) => method_name_from_literal(literal),
        _ => method_name_from_quote_constructor(expr),
    }
}

fn method_name_from_quote_constructor(expr: &Expr) -> Option<String> {
    let Expr::Builtin {
        name: BuiltinOp::ExprNew,
        args,
        ..
    } = expr
    else {
        return None;
    };
    if args.len() < 3 {
        return None;
    }
    let head = symbol_from_quote_constructor(&args[0])?;
    if head == "struct" {
        return symbol_from_quote_constructor(args.get(2)?).map(str::to_string);
    }
    if head == "=" {
        return method_name_from_call_constructor(&args[1]);
    }
    if head == "call" && args.len() >= 4 && symbol_from_quote_constructor(&args[1])? == "=" {
        return method_name_from_call_constructor(&args[2]);
    }
    None
}

fn method_name_from_call_constructor(expr: &Expr) -> Option<String> {
    let Expr::Builtin {
        name: BuiltinOp::ExprNew,
        args,
        ..
    } = expr
    else {
        return None;
    };
    if args.len() < 2 || symbol_from_quote_constructor(&args[0])? != "call" {
        return None;
    }
    symbol_from_quote_constructor(&args[1]).map(str::to_string)
}

fn symbol_from_quote_constructor(expr: &Expr) -> Option<&str> {
    match expr {
        Expr::Literal(crate::ir::core::Literal::Symbol(symbol), _) => Some(symbol.as_str()),
        Expr::Builtin {
            name: BuiltinOp::SymbolNew,
            args,
            ..
        } => match args.first()? {
            Expr::Literal(crate::ir::core::Literal::Str(symbol), _) => Some(symbol.as_str()),
            _ => None,
        },
        _ => None,
    }
}

fn method_name_from_literal(literal: &crate::ir::core::Literal) -> Option<String> {
    let crate::ir::core::Literal::Expr { head, args } = literal else {
        return None;
    };
    if head == "=" && args.len() >= 2 {
        return method_name_from_call_literal(&args[0]);
    }
    if head == "call" && args.len() >= 3 && literal_symbol(&args[0])? == "=" {
        return method_name_from_call_literal(&args[1]);
    }
    None
}

fn method_name_from_call_literal(literal: &crate::ir::core::Literal) -> Option<String> {
    let crate::ir::core::Literal::Expr { head, args } = literal else {
        return None;
    };
    if head == "call" {
        literal_symbol(args.first()?).map(str::to_string)
    } else {
        None
    }
}

fn literal_symbol(literal: &crate::ir::core::Literal) -> Option<&str> {
    match literal {
        crate::ir::core::Literal::Symbol(symbol) => Some(symbol.as_str()),
        _ => None,
    }
}

fn is_opaque_runtime_eval_name(function: &str) -> bool {
    matches!(
        function.rsplit('.').next().unwrap_or(function),
        "eval" | "include_string" | "evalfile"
    )
}

/// Phase 3: load stdlib modules for any using statements
/// that reference stdlib modules not already in program.modules.
fn load_stdlib_modules(
    program: &Program,
    opt_modules: &[crate::ir::core::Module],
) -> Vec<crate::ir::core::Module> {
    let existing_module_names: HashSet<String> =
        profile::time("compile.stdlib_existing_module_names", || {
            opt_modules.iter().map(|m| m.name.clone()).collect()
        });
    profile::time("compile.stdlib_load", || {
        // Collect all using imports from top-level and from within modules
        let mut all_usings: Vec<&UsingImport> = program.usings.iter().collect();

        for module in opt_modules {
            collect_module_usings_recursive(module, &mut all_usings);
        }

        // Use pure Rust stdlib loader for WASM builds
        let usings_to_load: Vec<UsingImport> = all_usings
            .iter()
            .filter(|u| !u.is_relative)
            .filter(|u| !existing_module_names.contains(&u.module))
            .filter(|u| !matches!(u.module.as_str(), "Base" | "Core" | "Main" | "Pkg"))
            .map(|u| (*u).clone())
            .collect();
        crate::stdlib_loader::load_stdlib_modules(&usings_to_load)
    })
}

fn resolve_using_module_name(
    using_import: &UsingImport,
    current_module_path: &str,
    module_functions: &HashMap<String, HashSet<String>>,
) -> Option<String> {
    if !using_import.is_relative {
        if let Some(base_submodule) = using_import.module.strip_prefix("Base.") {
            if module_functions.contains_key(base_submodule)
                && !super::constants::is_stdlib_module(base_submodule)
            {
                return Some(base_submodule.to_string());
            }
        }
        return Some(using_import.module.clone());
    }

    let relative_level = using_import.relative_level.max(1);
    let mut base_parts: Vec<&str> = if current_module_path.is_empty() {
        Vec::new()
    } else {
        current_module_path.split('.').collect()
    };

    let parent_hops = relative_level.saturating_sub(1).min(base_parts.len());
    for _ in 0..parent_hops {
        base_parts.pop();
    }

    let candidate = if base_parts.is_empty() {
        using_import.module.clone()
    } else {
        format!("{}.{}", base_parts.join("."), using_import.module)
    };

    if module_functions.contains_key(candidate.as_str()) {
        return Some(candidate);
    }

    // Julia permits parent modules to refer to themselves by name, e.g.
    // `import ..LinearAlgebra: inv` inside `LinearAlgebra.LAPACK`.
    if module_functions.contains_key(using_import.module.as_str()) {
        return Some(using_import.module.clone());
    }

    None
}

fn canonical_import_alias_source(
    using_import: &UsingImport,
    resolved_module: &str,
    source: &str,
) -> String {
    if source == using_import.module {
        resolved_module.to_string()
    } else if let Some(suffix) = source.strip_prefix(&using_import.module) {
        format!("{resolved_module}{suffix}")
    } else {
        source.to_string()
    }
}

fn validate_scope_using_imports(
    usings: &[UsingImport],
    module_functions: &HashMap<String, HashSet<String>>,
) -> CResult<()> {
    for using_import in usings {
        validate_using_import(using_import, module_functions)?;
    }
    Ok(())
}

fn validate_module_using_imports(
    module: &crate::ir::core::Module,
    module_functions: &HashMap<String, HashSet<String>>,
) -> CResult<()> {
    validate_scope_using_imports(&module.usings, module_functions)?;
    for submodule in &module.submodules {
        validate_module_using_imports(submodule, module_functions)?;
    }
    Ok(())
}

fn validate_using_import(
    using_import: &UsingImport,
    module_functions: &HashMap<String, HashSet<String>>,
) -> CResult<()> {
    if using_import.is_relative {
        return Ok(());
    }

    let Some(base_submodule) = using_import.module.strip_prefix("Base.") else {
        return Ok(());
    };

    if module_functions.contains_key(base_submodule)
        && !super::constants::is_stdlib_module(base_submodule)
    {
        return Ok(());
    }

    if module_functions.contains_key(using_import.module.as_str()) {
        return Ok(());
    }

    err(format!(
        "UndefVarError: `{base_submodule}` not defined in `Base`"
    ))
}

fn resolve_scope_using_imports(
    usings: &[UsingImport],
    current_module_path: &str,
    module_functions: &HashMap<String, HashSet<String>>,
) -> Vec<ResolvedUsingImport> {
    usings
        .iter()
        .enumerate()
        .filter_map(|(program_index, using_import)| {
            let module =
                resolve_using_module_name(using_import, current_module_path, module_functions)?;
            let selected_symbols = using_import
                .symbols
                .as_ref()
                .map(|names| names.iter().cloned().collect());
            let alias_assignments: Vec<_> = using_import
                .alias_bindings
                .iter()
                .map(|(source, alias)| {
                    let canonical_source =
                        canonical_import_alias_source(using_import, &module, source);
                    (
                        alias.clone(),
                        source.clone(),
                        canonical_source,
                        using_import.span,
                    )
                })
                .collect();
            Some(ResolvedUsingImport {
                program_index,
                module,
                source_module: using_import.module.clone(),
                is_import: using_import.is_import,
                is_relative: using_import.is_relative,
                selected_symbols,
                alias_bindings: using_import.alias_bindings.clone(),
                span: using_import.span,
                binds_module_root: using_import.symbols.is_none()
                    && using_import.alias_bindings.is_empty(),
                has_renames: !using_import.alias_bindings.is_empty(),
                alias_assignments,
            })
        })
        .collect()
}

fn register_live_import_bindings(
    bindings: &mut HashMap<String, String>,
    destination_module: &str,
    resolved_usings: &[ResolvedUsingImport],
    module_functions: &HashMap<String, HashSet<String>>,
    module_exports: &HashMap<String, HashSet<String>>,
) {
    use crate::compile::core_compiler::{build_live_import_binding_states, ModuleAliasState};

    let mut live_names: HashSet<&str> = resolved_usings
        .iter()
        .flat_map(|using| {
            using
                .alias_assignments
                .iter()
                .map(|(alias, _, _, _)| alias.as_str())
                .chain(
                    using
                        .selected_symbols
                        .iter()
                        .flat_map(|symbols| symbols.iter().map(String::as_str)),
                )
        })
        .collect();
    live_names.extend(
        module_exports
            .get(destination_module)
            .into_iter()
            .flat_map(|exports| exports.iter().map(String::as_str)),
    );
    live_names.extend(resolved_usings.iter().flat_map(|using| {
        (!using.is_import && using.selected_symbols.is_none() && using.binds_module_root)
            .then(|| module_exports.get(&using.module))
            .flatten()
            .into_iter()
            .flat_map(|exports| exports.iter().map(String::as_str))
    }));

    let states = build_live_import_binding_states(
        resolved_usings,
        module_functions,
        module_exports,
        bindings,
    );

    // A later canonicalization pass can turn an apparent provider conflict
    // into one shared binding, or vice versa. Remove this scope's prior result
    // before applying the newly resolved states so stale first-pass winners do
    // not survive a genuine ambiguity.
    for name in &live_names {
        bindings.remove(&format!("{destination_module}.{name}"));
    }

    for (name, state) in states {
        if !live_names.contains(name.as_str()) {
            continue;
        }
        let ModuleAliasState::Bound {
            canonical_target, ..
        } = state
        else {
            continue;
        };
        bindings.insert(format!("{destination_module}.{name}"), canonical_target);
    }
}

fn register_all_live_import_bindings(
    bindings: &mut HashMap<String, String>,
    scopes: &[(String, Vec<ResolvedUsingImport>)],
    module_functions: &HashMap<String, HashSet<String>>,
    module_exports: &HashMap<String, HashSet<String>>,
) {
    // Scope collection is not dependency ordered (and package re-exports can
    // form chains several modules deep). Seed every direct edge, then repeat
    // until canonical identities stop changing. The chain resolver itself is
    // cycle safe, so malformed/cyclic metadata converges to the uncollapsed
    // direct states rather than looping here.
    for _ in 0..=scopes.len() {
        let before = bindings.clone();
        for (destination_module, resolved_usings) in scopes {
            register_live_import_bindings(
                bindings,
                destination_module,
                resolved_usings,
                module_functions,
                module_exports,
            );
        }
        if *bindings == before {
            break;
        }
    }
}

pub(super) fn collect_live_import_bindings(program: &Program) -> HashMap<String, String> {
    let mut module_functions = HashMap::new();
    let mut module_exports = HashMap::new();
    let mut module_constants = HashMap::new();
    let mut module_usings = HashMap::new();
    for module in &program.modules {
        collect_module_info(
            module,
            "",
            &mut module_functions,
            &mut module_exports,
            &mut module_constants,
        );
        collect_module_usings(module, "", &mut module_usings);
    }

    let mut scopes = Vec::with_capacity(module_usings.len() + 1);
    let top_level = resolve_scope_using_imports(&program.usings, "", &module_functions);
    scopes.push(("Main".to_string(), top_level));
    let mut module_usings: Vec<_> = module_usings.into_iter().collect();
    module_usings.sort_by(|(left, _), (right, _)| left.cmp(right));
    for (module_path, usings) in module_usings {
        let resolved = resolve_scope_using_imports(&usings, &module_path, &module_functions);
        scopes.push((module_path, resolved));
    }
    let mut bindings = HashMap::new();
    register_all_live_import_bindings(&mut bindings, &scopes, &module_functions, &module_exports);
    bindings
}

fn collect_bare_module_paths(
    module: &crate::ir::core::Module,
    prefix: &str,
    paths: &mut HashSet<String>,
) {
    let path = if prefix.is_empty() {
        module.name.clone()
    } else {
        format!("{prefix}.{}", module.name)
    };
    if module.is_bare {
        paths.insert(path.clone());
    }
    for submodule in &module.submodules {
        collect_bare_module_paths(submodule, &path, paths);
    }
}

/// A `let` whose bindings are all synthetic scope markers introduces no
/// user-visible local (bare `begin`/`@testset`/`@time` bodies lower this way),
/// so it is not a capturable binding scope — mirrors
/// `lowering::closure_box::is_scope_marker_let`.
fn is_scope_marker_let_bindings(bindings: &[(crate::ir::core::InternedStr, Expr)]) -> bool {
    bindings
        .iter()
        .all(|(name, _)| name.starts_with("__sjulia_let_scope_"))
}

/// Result of [`collect_let_scope_function_captures`]: the capture set of every
/// function defined in a module-level `let` scope, and the subset of them that
/// were declared `global` (their closure value binds to the module-level name).
#[derive(Default)]
struct LetScopeCaptures {
    captures: HashMap<String, HashSet<String>>,
    global_closures: HashSet<String>,
}

/// Record the captures of every named function defined inside a module-level
/// hard scope: a real-binding `let` or macro-expanded `@testset` (Issues
/// #11015, #11260).
///
/// `enclosing` holds the locals of all enclosing hard scopes and is empty at
/// module top level, where names are globals a function must keep reading
/// dynamically rather than capturing. Function bodies are not descended: a
/// function nested inside a function is handled by the parent-relative capture
/// analysis in `CoreCompiler::compile_stmt` / `prepopulate_closure_captures`.
fn collect_let_scope_function_captures(
    stmts: &[Stmt],
    enclosing: &HashSet<String>,
    out: &mut LetScopeCaptures,
) {
    for stmt in stmts {
        match stmt {
            Stmt::Expr {
                expr: Expr::LetBlock { bindings, body, .. },
                ..
            }
            | Stmt::Assign {
                value: Expr::LetBlock { bindings, body, .. },
                ..
            } => {
                let mut scope = enclosing.clone();
                if !is_scope_marker_let_bindings(bindings) {
                    scope.extend(bindings.iter().map(|(name, _)| name.to_string()));
                }
                collect_hard_scope_function_captures(&body.stmts, &mut scope, out);
            }
            Stmt::Block(block) | Stmt::Timed { body: block, .. } => {
                collect_let_scope_function_captures(&block.stmts, enclosing, out);
            }
            Stmt::TestSet { body, .. } => {
                let mut scope = enclosing.clone();
                collect_hard_scope_function_captures(&body.stmts, &mut scope, out);
            }
            Stmt::For { body, .. }
            | Stmt::ForEach { body, .. }
            | Stmt::ForEachTuple { body, .. }
            | Stmt::While { body, .. } => {
                collect_let_scope_function_captures(&body.stmts, enclosing, out);
            }
            Stmt::If {
                then_branch,
                else_branch,
                ..
            } => {
                collect_let_scope_function_captures(&then_branch.stmts, enclosing, out);
                if let Some(eb) = else_branch {
                    collect_let_scope_function_captures(&eb.stmts, enclosing, out);
                }
            }
            Stmt::Try {
                try_block,
                catch_block,
                else_block,
                finally_block,
                ..
            } => {
                collect_let_scope_function_captures(&try_block.stmts, enclosing, out);
                for block in [catch_block, else_block, finally_block]
                    .into_iter()
                    .flatten()
                {
                    collect_let_scope_function_captures(&block.stmts, enclosing, out);
                }
            }
            Stmt::Assign { .. }
            | Stmt::AddAssign { .. }
            | Stmt::Break { .. }
            | Stmt::Continue { .. }
            | Stmt::Return { .. }
            | Stmt::Expr { .. }
            | Stmt::Meta { .. }
            | Stmt::Test { .. }
            | Stmt::TestThrows { .. }
            | Stmt::IndexAssign { .. }
            | Stmt::FieldAssign { .. }
            | Stmt::DestructuringAssign { .. }
            | Stmt::DictAssign { .. }
            | Stmt::Using { .. }
            | Stmt::Export { .. }
            | Stmt::FunctionDef { .. }
            | Stmt::EvalFunctionDef { .. }
            | Stmt::Label { .. }
            | Stmt::Goto { .. }
            | Stmt::EnumDef { .. }
            | Stmt::RuntimeNominalDef { .. }
            | Stmt::Global { .. }
            | Stmt::LocalDecl { .. } => {}
        }
    }
}

/// Walk one already-entered hard scope using this capture-liveness transfer
/// contract (Issue #11278):
///
/// - A statement sequence mutates `live` in execution order. Only stores before
///   a function definition are offered as captures; lexical predeclaration is a
///   separate compiler concern, and cannot supply a value at closure creation
///   (Issue #11249).
/// - `if` is scope-transparent. A dispatch-free constant condition transfers
///   only its taken arm; an unknown condition intersects the two successor live
///   sets (a missing `else` is the unchanged incoming set).
/// - Loops may execute zero times, and `let`/`@testset` are hard children, so
///   each child starts from a clone and its newly-live bindings are discarded at
///   the boundary. Functions inside the child still observe its binders.
/// - `try`, `catch`, `else`, and `finally` are independent hard scopes. Every
///   clause starts from the same incoming live set; no clause feeds a sibling or
///   the continuation.
/// - A catch binder is added only to the catch child's live set. It is available
///   to functions defined inside that catch, but dies at the clause boundary;
///   an outer binding with the same name remains live after the clause.
///
/// Keep the match exhaustive: a new [`Stmt`] variant is a compile-time review
/// point whose lexical boundary and control-flow transfer must be classified.
fn collect_hard_scope_function_captures(
    stmts: &[Stmt],
    live: &mut HashSet<String>,
    out: &mut LetScopeCaptures,
) {
    // `global function f(...)` lowers to `Block([Global{[f]}, FunctionDef(f)])`
    // (Issue #11015), so the marker sits in the same statement list as the
    // definition it applies to.
    let mut declared_globals: HashSet<&str> = HashSet::new();
    for stmt in stmts {
        if let Stmt::Global { names, .. } = stmt {
            declared_globals.extend(names.iter().map(String::as_str));
        }
    }
    for stmt in stmts {
        match stmt {
            Stmt::FunctionDef { func, .. } | Stmt::EvalFunctionDef { func, .. } => {
                if live.is_empty() {
                    continue;
                }
                let caps = analyze_free_variables(func, live);
                if !caps.is_empty() {
                    if declared_globals.contains(func.name.as_str()) {
                        out.global_closures.insert(func.name.clone());
                    }
                    out.captures
                        .entry(func.name.clone())
                        .or_default()
                        .extend(caps);
                }
            }
            Stmt::Expr {
                expr: Expr::LetBlock { bindings, body, .. },
                ..
            }
            | Stmt::Assign {
                value: Expr::LetBlock { bindings, body, .. },
                ..
            } => {
                let mut scope = live.clone();
                if !is_scope_marker_let_bindings(bindings) {
                    scope.extend(bindings.iter().map(|(name, _)| name.to_string()));
                }
                collect_hard_scope_function_captures(&body.stmts, &mut scope, out);
                if let Stmt::Assign { var, .. } = stmt {
                    live.insert(var.clone());
                }
            }
            Stmt::Assign { var, .. } | Stmt::AddAssign { var, .. } => {
                live.insert(var.clone());
            }
            Stmt::DestructuringAssign { targets, .. } => {
                live.extend(targets.iter().cloned());
            }
            Stmt::Block(block) | Stmt::Timed { body: block, .. } => {
                collect_hard_scope_function_captures(&block.stmts, live, out);
            }
            Stmt::TestSet { body, .. } => {
                let mut child = live.clone();
                collect_hard_scope_function_captures(&body.stmts, &mut child, out);
            }
            Stmt::For { var, body, .. } | Stmt::ForEach { var, body, .. } => {
                let mut child = live.clone();
                child.insert(var.clone());
                collect_hard_scope_function_captures(&body.stmts, &mut child, out);
            }
            Stmt::ForEachTuple { vars, body, .. } => {
                let mut child = live.clone();
                child.extend(vars.iter().cloned());
                collect_hard_scope_function_captures(&body.stmts, &mut child, out);
            }
            Stmt::While { body, .. } => {
                let mut child = live.clone();
                collect_hard_scope_function_captures(&body.stmts, &mut child, out);
            }
            Stmt::If {
                condition,
                then_branch,
                else_branch,
                ..
            } => {
                let mut then_live = live.clone();
                collect_hard_scope_function_captures(&then_branch.stmts, &mut then_live, out);
                let mut else_live = live.clone();
                if let Some(eb) = else_branch {
                    collect_hard_scope_function_captures(&eb.stmts, &mut else_live, out);
                }
                match super::stmt::const_bool_condition_with_lookup(condition, &|_| None) {
                    Some(true) => *live = then_live,
                    Some(false) => *live = else_live,
                    None => {
                        then_live.retain(|name| else_live.contains(name));
                        *live = then_live;
                    }
                }
            }
            Stmt::Try {
                try_block,
                catch_var,
                catch_block,
                else_block,
                finally_block,
                ..
            } => {
                // Julia gives every try/catch/else/finally clause its own hard
                // scope. Walk each child from the same enclosing live set so a
                // function inside that clause can capture preceding clause
                // locals, but never propagate newly introduced names into a
                // sibling clause or the continuation after `end`.
                let mut try_live = live.clone();
                collect_hard_scope_function_captures(&try_block.stmts, &mut try_live, out);

                if let Some(block) = catch_block {
                    let mut catch_live = live.clone();
                    if let Some(var) = catch_var {
                        catch_live.insert(var.clone());
                    }
                    collect_hard_scope_function_captures(&block.stmts, &mut catch_live, out);
                }

                if let Some(block) = else_block {
                    let mut else_live = live.clone();
                    collect_hard_scope_function_captures(&block.stmts, &mut else_live, out);
                }

                if let Some(block) = finally_block {
                    let mut finally_live = live.clone();
                    collect_hard_scope_function_captures(&block.stmts, &mut finally_live, out);
                }
            }
            Stmt::Break { .. }
            | Stmt::Continue { .. }
            | Stmt::Return { .. }
            | Stmt::Expr { .. }
            | Stmt::Meta { .. }
            | Stmt::Test { .. }
            | Stmt::TestThrows { .. }
            | Stmt::IndexAssign { .. }
            | Stmt::FieldAssign { .. }
            | Stmt::DictAssign { .. }
            | Stmt::Using { .. }
            | Stmt::Export { .. }
            | Stmt::Label { .. }
            | Stmt::Goto { .. }
            | Stmt::EnumDef { .. }
            | Stmt::RuntimeNominalDef { .. }
            | Stmt::Global { .. }
            | Stmt::LocalDecl { .. } => {}
        }
    }
}

#[cfg(test)]
mod hard_scope_capture_test_support {
    use super::*;

    pub(super) struct CaptureCase {
        pub(super) name: &'static str,
        pub(super) shape: &'static str,
        pub(super) captured_name: &'static str,
        pub(super) module_body_captures: bool,
        pub(super) module_creator_captures: bool,
        pub(super) nested_body_captures: bool,
        pub(super) nested_creator_captures: bool,
    }

    fn paired_source(case: &CaptureCase) -> (String, String, String) {
        let module_function = format!("module_{}", case.name);
        let nested_function = format!("nested_{}", case.name);
        let outer_function = format!("outer_{}", case.name);
        let module_shape = case.shape.replace("$FUNCTION", &module_function);
        let nested_shape = case.shape.replace("$FUNCTION", &nested_function);
        let source = format!(
            "using Test\nlet seed = 0\n{module_shape}\nend\n\
             function {outer_function}()\nseed = 0\n{nested_shape}\nnothing\nend\n"
        );
        (
            source,
            module_function,
            format!("{outer_function}#{nested_function}"),
        )
    }

    fn assert_capture_bytecode(
        compiled: &CompiledProgram,
        function_name: &str,
        captured_name: &str,
        body_captures: bool,
        creator_captures: bool,
        case_name: &str,
    ) -> Result<(), String> {
        let function = compiled
            .functions
            .iter()
            .find(|function| function.name == function_name)
            .ok_or_else(|| format!("{case_name}: missing compiled function {function_name}"))?;
        let body = &compiled.code[function.code_start..function.code_end];
        let has_captured_load = body
            .iter()
            .any(|instr| matches!(instr, Instr::LoadCaptured(name) if name == captured_name));
        let has_dynamic_load = body
            .iter()
            .any(|instr| matches!(instr, Instr::LoadAny(name) if name == captured_name));
        let creator_capture_names = compiled.code.iter().find_map(|instr| match instr {
            Instr::CreateClosure {
                func_name,
                capture_names,
            } if func_name == function_name => Some(capture_names),
            Instr::CreateResolvedClosure(operands) if operands.name == function_name => {
                Some(&operands.capture_names)
            }
            _ => None,
        });

        if body_captures {
            if !has_captured_load || has_dynamic_load {
                return Err(format!(
                    "{case_name}/{function_name}: expected LoadCaptured({captured_name}) and no \
                     LoadAny; body={body:?}"
                ));
            }
        } else if has_captured_load || !has_dynamic_load {
            return Err(format!(
                "{case_name}/{function_name}: expected LoadAny({captured_name}) and no \
                 LoadCaptured; body={body:?}"
            ));
        }

        if creator_captures {
            let Some(capture_names) = creator_capture_names else {
                return Err(format!(
                    "{case_name}/{function_name}: creator must capture {captured_name}, but no \
                     CreateClosure was emitted"
                ));
            };
            if !capture_names.iter().any(|name| name == captured_name) {
                return Err(format!(
                    "{case_name}/{function_name}: creator capture_names={capture_names:?} omits \
                     {captured_name}"
                ));
            }
        } else if creator_capture_names
            .is_some_and(|capture_names| capture_names.iter().any(|name| name == captured_name))
        {
            return Err(format!(
                "{case_name}/{function_name}: creator must not capture {captured_name}; \
                 capture_names={creator_capture_names:?}"
            ));
        }
        Ok(())
    }

    pub(super) fn captures(src: &str, function: &str) -> Result<HashSet<String>, String> {
        let program = crate::pipeline::parse_and_lower(src).map_err(|err| format!("{err:?}"))?;
        let mut out = LetScopeCaptures::default();
        collect_let_scope_function_captures(&program.main.stmts, &HashSet::new(), &mut out);
        Ok(out.captures.remove(function).unwrap_or_default())
    }

    pub(super) fn run_cases(cases: &[CaptureCase]) -> Result<(), String> {
        for case in cases {
            let (source, module_function, nested_function) = paired_source(case);
            let program = crate::pipeline::parse_and_lower(&source)
                .map_err(|err| format!("{}: lower failed: {err:?}", case.name))?;
            let mut module_captures = LetScopeCaptures::default();
            collect_let_scope_function_captures(
                &program.main.stmts,
                &HashSet::new(),
                &mut module_captures,
            );
            let module_has_capture = module_captures
                .captures
                .get(&module_function)
                .is_some_and(|captures| captures.contains(case.captured_name));
            if module_has_capture != case.module_body_captures {
                return Err(format!(
                    "{}: module pre-analysis captures={:?}, expected {} for {}",
                    case.name,
                    module_captures.captures.get(&module_function),
                    case.module_body_captures,
                    case.captured_name
                ));
            }

            let compiled = crate::compile::compile_core_program(&program)
                .map_err(|err| format!("{}: compile failed: {err:?}", case.name))?;
            assert_capture_bytecode(
                &compiled,
                &module_function,
                case.captured_name,
                case.module_body_captures,
                case.module_creator_captures,
                case.name,
            )?;
            assert_capture_bytecode(
                &compiled,
                &nested_function,
                case.captured_name,
                case.nested_body_captures,
                case.nested_creator_captures,
                case.name,
            )?;
        }
        Ok(())
    }
}

#[cfg(test)]
mod hard_scope_capture_liveness_tests {
    use super::hard_scope_capture_test_support::{run_cases, CaptureCase};

    #[test]
    fn module_and_nested_capture_paths_share_control_flow_contract_issue_11278(
    ) -> Result<(), String> {
        let cases = [
            CaptureCase {
                name: "sequence",
                shape: "x = 1\n$FUNCTION() = x",
                captured_name: "x",
                module_body_captures: true,
                module_creator_captures: true,
                nested_body_captures: true,
                nested_creator_captures: true,
            },
            CaptureCase {
                name: "constant_if",
                shape: "if true\n    x = 1\nend\n$FUNCTION() = x",
                captured_name: "x",
                module_body_captures: true,
                module_creator_captures: true,
                nested_body_captures: true,
                nested_creator_captures: true,
            },
            CaptureCase {
                name: "unknown_if_both",
                shape: "if seed == 0\n    x = 1\nelse\n    x = 2\nend\n$FUNCTION() = x",
                captured_name: "x",
                module_body_captures: true,
                module_creator_captures: true,
                nested_body_captures: true,
                nested_creator_captures: true,
            },
            CaptureCase {
                name: "unknown_if_one",
                shape: "if seed == 0\n    x = 1\nend\n$FUNCTION() = x",
                captured_name: "x",
                // Module hard-scope pre-analysis uses definite assignment, so
                // the body stays dynamic. Creator-side lexical prescan is
                // deliberately conservative and snapshots the slot if present.
                // A nested function lives in its parent's lexical local scope,
                // where `x` is a local cell even when this path leaves it
                // undefined at runtime.
                module_body_captures: false,
                module_creator_captures: true,
                nested_body_captures: true,
                nested_creator_captures: true,
            },
            CaptureCase {
                name: "zero_trip_loop",
                shape: "while false\n    x = 1\nend\n$FUNCTION() = x",
                captured_name: "x",
                module_body_captures: false,
                // A hard-scope loop owns a newly assigned name when no outer
                // local exists. The name is therefore not visible to the
                // following function on either path, matching upstream Julia.
                module_creator_captures: false,
                nested_body_captures: false,
                nested_creator_captures: false,
            },
            CaptureCase {
                name: "hard_child",
                shape: "@testset \"child\" begin\n    x = 1\nend\n$FUNCTION() = x",
                captured_name: "x",
                module_body_captures: false,
                module_creator_captures: false,
                nested_body_captures: false,
                nested_creator_captures: false,
            },
            CaptureCase {
                name: "try_clause_local",
                shape: "try\n    x = 1\ncatch\nend\n$FUNCTION() = x",
                captured_name: "x",
                module_body_captures: false,
                // A try-clause local is out of scope after the clause on both
                // module and nested-function paths (upstream #11281 parity).
                module_creator_captures: false,
                nested_body_captures: false,
                nested_creator_captures: false,
            },
            CaptureCase {
                name: "else_cannot_see_try_local",
                shape: "try\n    x = 1\ncatch\nelse\n    $FUNCTION() = x\nend",
                captured_name: "x",
                module_body_captures: false,
                module_creator_captures: false,
                nested_body_captures: false,
                nested_creator_captures: false,
            },
            CaptureCase {
                name: "else_clause_local_after",
                shape: "try\n    nothing\ncatch\nelse\n    x = 1\nend\n$FUNCTION() = x",
                captured_name: "x",
                // The else-clause local is discarded at the clause boundary;
                // later functions must resolve `x` dynamically and fail if no
                // outer binding exists.
                module_body_captures: false,
                module_creator_captures: false,
                nested_body_captures: false,
                nested_creator_captures: false,
            },
            CaptureCase {
                name: "catch_binder_inside",
                shape: "try\n    error(\"boom\")\ncatch e\n    $FUNCTION() = e\nend",
                captured_name: "e",
                module_body_captures: true,
                module_creator_captures: true,
                // A function created inside the catch clause closes over the
                // live binder on both module and nested-function paths.
                nested_body_captures: true,
                nested_creator_captures: true,
            },
            CaptureCase {
                name: "catch_binder_after",
                shape: "try\n    error(\"boom\")\ncatch e\nend\n$FUNCTION() = e",
                captured_name: "e",
                module_body_captures: false,
                module_creator_captures: false,
                nested_body_captures: false,
                nested_creator_captures: false,
            },
            CaptureCase {
                name: "finally_clause_local",
                shape: "try\n    nothing\nfinally\n    x = 1\nend\n$FUNCTION() = x",
                captured_name: "x",
                module_body_captures: false,
                module_creator_captures: false,
                nested_body_captures: false,
                nested_creator_captures: false,
            },
            CaptureCase {
                name: "later_assignment_strict_negative",
                shape: "$FUNCTION() = x\nx = 1",
                captured_name: "x",
                // Unlike conservative control-flow prescans, source ordering
                // gives both paths a strict negative: no value exists when the
                // closure/function definition is created (Issue #11249).
                module_body_captures: false,
                module_creator_captures: false,
                nested_body_captures: false,
                nested_creator_captures: false,
            },
        ];

        run_cases(&cases)
    }
}

#[cfg(test)]
mod hard_scope_capture_liveness_tests_regressions {
    use super::hard_scope_capture_test_support::captures;
    use super::*;

    #[test]
    fn constant_if_assignment_is_live_after_taken_branch_issue_11260() {
        let actual = captures(
            "let anchor = 0\nif true\n    x = 41\nend\nf() = x + 1\nend\n",
            "f",
        );
        assert_eq!(actual, Ok(HashSet::from(["x".to_string()])));
    }

    #[test]
    fn unknown_if_keeps_only_bindings_live_on_both_paths_issue_11260() {
        let both = captures(
            "let anchor = 0\nif anchor == 0\n    x = 41\nelse\n    x = 42\nend\nf() = x + 1\nend\n",
            "f",
        );
        assert_eq!(both, Ok(HashSet::from(["x".to_string()])));

        let one = captures(
            "let anchor = 0\nif anchor == 0\n    x = 41\nend\nf() = x + 1\nend\n",
            "f",
        );
        assert_eq!(one.map(|captures| captures.contains("x")), Ok(false));
    }

    #[test]
    fn catch_binder_is_live_inside_catch_issue_11260() {
        let actual = captures(
            "let anchor = 0\ntry\n    error(\"boom\")\ncatch e\n    f() = e\nend\nend\n",
            "f",
        );
        assert_eq!(actual, Ok(HashSet::from(["e".to_string()])));
    }

    #[test]
    fn catch_binder_shadow_does_not_kill_outer_binding_issue_11260() {
        let actual = captures(
            "let e = 1\ntry\n    error(\"boom\")\ncatch e\n    nothing\nend\nf() = e\nend\n",
            "f",
        );
        assert_eq!(actual, Ok(HashSet::from(["e".to_string()])));
    }

    #[test]
    fn try_local_does_not_escape_clause_issue_11260() {
        let actual = captures(
            "let anchor = 0\ntry\n    x = 1\ncatch\n    x = 2\nend\nf() = x\nend\n",
            "f",
        );
        assert_eq!(actual.map(|captures| captures.contains("x")), Ok(false));
    }

    #[test]
    fn else_cannot_capture_try_local_issue_11260() {
        let actual = captures(
            "let anchor = 0\ntry\n    x = 1\ncatch\nelse\n    f() = x\nend\nend\n",
            "f",
        );
        assert_eq!(actual.map(|captures| captures.contains("x")), Ok(false));
    }

    #[test]
    fn catch_locals_do_not_escape_clause_issue_11260() {
        let local = captures(
            "let anchor = 0\ntry\n    error(\"boom\")\ncatch e\n    x = 1\nend\nf() = x\nend\n",
            "f",
        );
        assert_eq!(local.map(|captures| captures.contains("x")), Ok(false));

        let binder = captures(
            "let anchor = 0\ntry\n    error(\"boom\")\ncatch e\n    nothing\nend\nf() = e\nend\n",
            "f",
        );
        assert_eq!(binder.map(|captures| captures.contains("e")), Ok(false));
    }

    #[test]
    fn finally_local_does_not_escape_clause_issue_11260() {
        let actual = captures(
            "let anchor = 0\ntry\n    nothing\nfinally\n    x = 1\nend\nf() = x\nend\n",
            "f",
        );
        assert_eq!(actual.map(|captures| captures.contains("x")), Ok(false));
    }

    #[test]
    fn later_assignment_is_not_offered_to_earlier_function_issue_11249() {
        let actual = captures("let anchor = 0\nf() = x + 1\nx = 41\nend\n", "f");
        assert_eq!(actual.map(|captures| captures.contains("x")), Ok(false));
    }
}

fn block_contains_function_def(block: &Block) -> bool {
    block.stmts.iter().any(stmt_contains_function_def)
}

fn stmt_contains_function_def(stmt: &Stmt) -> bool {
    match stmt {
        Stmt::FunctionDef { .. } | Stmt::EvalFunctionDef { .. } => true,
        Stmt::Block(block)
        | Stmt::Timed { body: block, .. }
        | Stmt::TestSet { body: block, .. } => block_contains_function_def(block),
        Stmt::For { body, .. }
        | Stmt::ForEach { body, .. }
        | Stmt::ForEachTuple { body, .. }
        | Stmt::While { body, .. } => block_contains_function_def(body),
        Stmt::If {
            then_branch,
            else_branch,
            ..
        } => {
            block_contains_function_def(then_branch)
                || else_branch
                    .as_ref()
                    .is_some_and(block_contains_function_def)
        }
        Stmt::Try {
            try_block,
            catch_block,
            else_block,
            finally_block,
            ..
        } => {
            block_contains_function_def(try_block)
                || catch_block
                    .as_ref()
                    .is_some_and(block_contains_function_def)
                || else_block.as_ref().is_some_and(block_contains_function_def)
                || finally_block
                    .as_ref()
                    .is_some_and(block_contains_function_def)
        }
        _ => false,
    }
}

/// Phase 4: collect inline (nested) functions from top-level statements and
/// function bodies, with parent function tracking. `module_scope_overrides`
/// (`inline_functions` collection index -> owning module path, Issue
/// #10214/#10236) is populated for functions found directly inside a
/// module-body `let`/`@testset` (Issue #10073), so `build_function_universe`
/// can register them at the enclosing module's scope instead of `None`/Main.
fn collect_top_level_inline_functions(
    program: &Program,
    base_function_count: usize,
    opt_user_functions: &[Function],
    opt_main: &Block,
    all_modules: &[&crate::ir::core::Module],
    module_scope_overrides: &mut HashMap<usize, String>,
    base_lifted_inline_indices: &mut HashSet<usize>,
) -> Vec<(Function, Option<String>)> {
    profile::time("compile.collect_inline_functions", || {
        let mut inline_functions = Vec::new();

        if base_function_count > 0 && opt_main.stmts.iter().any(is_base_user_main_boundary) {
            let mut in_base_main = true;
            for stmt in &opt_main.stmts {
                if is_base_user_main_boundary(stmt) {
                    in_base_main = false;
                    continue;
                }
                let before = inline_functions.len();
                collect_stmt_functions(stmt, &mut inline_functions, None);
                if in_base_main {
                    base_lifted_inline_indices.extend(before..inline_functions.len());
                }
            }
        } else {
            if base_function_count > 0 {
                // `opt_main` is normally the optimized user segment only.
                // Recover Base/prelude main lifted helpers from the merged
                // program's main prefix so Issue #10211 can reuse the cached
                // `FunctionInfo`s without relying on unsafe positional matches.
                for stmt in &program.main.stmts {
                    if is_base_user_main_boundary(stmt) {
                        break;
                    }
                    let before = inline_functions.len();
                    collect_stmt_functions(stmt, &mut inline_functions, None);
                    base_lifted_inline_indices.extend(before..inline_functions.len());
                }
            }
            for stmt in &opt_main.stmts {
                collect_stmt_functions(stmt, &mut inline_functions, None);
            }
        }

        // Also collect from each top-level function's body. Keep scanning
        // Base bodies on the cached path because some cached Base entries
        // still rely on nested-function alignment and closure metadata.
        for func in program.functions.iter().take(base_function_count) {
            let before = inline_functions.len();
            collect_block_functions_with_new_authority(
                &func.body,
                &mut inline_functions,
                Some(&func.name),
                func.new_struct_name.as_deref(),
            );
            base_lifted_inline_indices.extend(before..inline_functions.len());
        }
        for func in opt_user_functions {
            collect_block_functions_with_new_authority(
                &func.body,
                &mut inline_functions,
                Some(&func.name),
                func.new_struct_name.as_deref(),
            );
        }
        // Also collect from module functions
        for module in all_modules {
            collect_from_module(module, "", &mut inline_functions, module_scope_overrides);
        }
        inline_functions
    })
}

/// Prevention coverage for Issue #9998 (root cause of #9787/#9990): when
/// `repl_current_function_count` is set, only that many structurally identified
/// source methods from the current fragment may receive
/// `visible_from_source_start` / delayed `min_world` activation. Marker-less
/// helpers do not consume the count, and prior already-executed methods remain
/// immediately visible (`min_world == 1`).
#[cfg(test)]
mod repl_hof_helper_9784_tests {
    use super::*;

    #[test]
    fn current_source_indices_skip_helpers_before_between_and_after_9784() -> Result<(), String> {
        let program = crate::pipeline::parse_and_lower(
            "source_first_9784(x) = x + 1\nsource_prior_9784(x) = x + 2",
        )
        .map_err(|error| format!("parse/lower failed: {error:?}"))?;
        let source_first = program
            .functions
            .iter()
            .find(|function| function.name == "source_first_9784")
            .cloned()
            .ok_or_else(|| "missing first source function".to_string())?;
        let source_prior = program
            .functions
            .iter()
            .find(|function| function.name == "source_prior_9784")
            .cloned()
            .ok_or_else(|| "missing prior source function".to_string())?;
        let mut helper_before = source_first.as_ref().clone();
        helper_before.name = "__helper_before_9784".to_string();
        helper_before = helper_before.into_lowering_helper();
        let mut helper_between = helper_before.clone();
        helper_between.name = "__helper_between_9784".to_string();
        let mut helper_after = helper_before.clone();
        helper_after.name = "__helper_after_9784".to_string();
        let all_functions = vec![
            (&helper_before, None),
            (source_first.as_ref(), None),
            (&helper_between, None),
            (source_prior.as_ref(), None),
            (&helper_after, None),
        ];

        let indices = repl_current_input_source_function_indices(
            &all_functions,
            0,
            all_functions.len(),
            &HashMap::new(),
            Some(1),
        )
        .ok_or_else(|| "missing explicit current-input source set".to_string())?;
        assert_eq!(indices, HashSet::from([1]));
        Ok(())
    }

    /// Marker-less lowering helpers are not source definitions and therefore
    /// must not consume the current-input source-method budget. This exercises
    /// `helper -> primary -> helper -> prior method` (Issue #9784).
    #[test]
    fn repl_current_function_count_skips_interposed_helpers_issue_9784() {
        let worlds = super::repl_source_order_boundary_tests::compile_min_worlds(
            "map(x -> x + 1, [1])\nf(x) = x + 1\nntuple(i -> i, 1)\ng(x) = x + 2\n",
            Some(1),
        );
        assert_eq!(
            worlds,
            vec![u64::MAX, 1],
            "the first source method must be current even when marker-less helpers precede or \
             follow it; the later prior method must stay active, got {worlds:?}"
        );
    }
}

#[cfg(test)]
#[allow(clippy::unwrap_used, clippy::expect_used)]
mod repl_source_order_boundary_tests {
    use super::*;

    /// Compile `src` (must define exactly the two bare top-level functions
    /// `f` and `g`, in that source order, and nothing else user-defined) with
    /// an explicit `repl_current_function_count` (mirrors
    /// `repl_full_compile`'s `CompilerCacheInput`, minus the Base cache — the
    /// boundary logic under test does not depend on the Base cache being
    /// active) and return `[f.min_world, g.min_world]`.
    ///
    /// Looked up BY NAME rather than by positional index/skip-count: the
    /// merged Base prelude also contributes `FunctionInfo` entries with
    /// `min_world == u64::MAX` (e.g. runtime-eval-eligible Base functions),
    /// so a position-based skip of `base_function_count` cannot reliably
    /// isolate the two user functions under test.
    pub(super) fn compile_min_worlds(
        src: &str,
        repl_current_function_count: Option<usize>,
    ) -> Vec<u64> {
        let program = crate::pipeline::parse_and_lower(src).expect("parse/lower");
        let output = compile_core_program_internal(
            &program,
            &HashMap::new(),
            &HashMap::new(),
            CompilerCacheInput {
                repl_current_function_count,
                ..Default::default()
            },
        )
        .expect("compile");
        ["f", "g"]
            .iter()
            .map(|name| {
                output
                    .compiled
                    .functions
                    .iter()
                    .find(|f| f.name == *name)
                    .unwrap_or_else(|| panic!("expected a compiled function named {name:?}"))
                    .min_world
            })
            .collect()
    }

    /// Baseline (Issue #9650): an ordinary script with NO
    /// `repl_current_function_count` delays EVERY root-source top-level
    /// function to `u64::MAX` — source-order activation applies uniformly
    /// when there is no REPL current-input/prior split.
    #[test]
    fn ordinary_script_delays_every_top_level_function_issue_9650() {
        let worlds = compile_min_worlds("f(x) = x + 1\ng(x) = x + 2\n", None);
        assert_eq!(
            worlds.len(),
            2,
            "expected exactly f and g as user functions, got {worlds:?}"
        );
        assert!(
            worlds.iter().all(|&w| w == u64::MAX),
            "ordinary script (no repl_current_function_count) must delay every \
             root-source top-level function to u64::MAX, got {worlds:?}"
        );
    }

    /// Regression boundary for Issue #9998 (root cause of #9787): a REPL
    /// full compile's `program.functions` is `[current-input functions ...,
    /// prior-eval functions merge_definitions appended AFTER them]`. Only
    /// the selected `repl_current_function_count` source methods may be delayed;
    /// functions outside that structural set (the merged-after prior methods) must
    /// keep `min_world == 1` so a fresh Persistent VM sees them immediately,
    /// instead of treating an already-executed method as not-yet-visible.
    #[test]
    fn repl_current_function_count_bounds_delayed_activation_issue_9998() {
        // `f` simulates the current input's own function (index 0); `g`
        // simulates a prior-eval method `merge_definitions` appended AFTER
        // it (Issue #9787's REPL full-compile merge order).
        let worlds = compile_min_worlds("f(x) = x + 1\ng(x) = x + 2\n", Some(1));
        assert_eq!(
            worlds.len(),
            2,
            "expected exactly f and g as user functions, got {worlds:?}"
        );
        assert_eq!(
            worlds[0],
            u64::MAX,
            "current-input function (leading, within repl_current_function_count) \
             must still get delayed source-order activation, got {worlds:?}"
        );
        assert_eq!(
            worlds[1], 1,
            "prior-eval function merged AFTER the current input's functions must \
             stay immediately visible (min_world == 1), not delayed — \
             Issue #9787/#9998, got {worlds:?}"
        );
    }

    /// `repl_current_function_count == Some(0)`: a definition-only REPL eval
    /// whose current input defines NO new top-level functions (e.g. a bare
    /// expression eval that only merged prior definitions) must leave every
    /// merged-in prior function immediately visible — none delayed.
    #[test]
    fn repl_current_function_count_zero_delays_nothing_issue_9998() {
        let worlds = compile_min_worlds("f(x) = x + 1\ng(x) = x + 2\n", Some(0));
        assert_eq!(
            worlds,
            vec![1, 1],
            "with zero current-input functions every merged-in function must stay \
             immediately visible, got {worlds:?}"
        );
    }
}

/// Coverage for [`classify_program_complexity`] (Issue #10127): each of the
/// three [`ProgramComplexity`] outcomes, plus the boundary between them, on
/// real parsed `Program`s (not hand-built IR literals — this classifier
/// reads whole-program shape, not a single expression).
#[cfg(test)]
#[allow(clippy::unwrap_used, clippy::expect_used)]
mod program_complexity_tests {
    use super::*;

    fn classify(src: &str) -> ProgramComplexity {
        let program = crate::pipeline::parse_and_lower(src).expect("parse/lower");
        classify_program_complexity(&program)
    }

    #[test]
    fn single_literal_call_is_trivial() {
        assert_eq!(
            classify(r#"println("Hello World")"#),
            ProgramComplexity::Trivial
        );
    }

    #[test]
    fn single_literal_return_is_trivial() {
        assert_eq!(classify("return 42"), ProgramComplexity::Trivial);
    }

    #[test]
    fn empty_program_is_simple_not_trivial() {
        // Zero statements is not "a single simple call", so it falls to Simple
        // rather than being misclassified as the narrower Trivial case.
        assert_eq!(classify(""), ProgramComplexity::Simple);
    }

    #[test]
    fn variable_assignment_is_simple() {
        assert_eq!(classify("x = 1\nprintln(x)"), ProgramComplexity::Simple);
    }

    #[test]
    fn call_with_variable_argument_is_simple_not_trivial() {
        // The call argument is a name, not a literal, so this is NOT the
        // "nothing to resolve beyond constant folding" Trivial shape.
        assert_eq!(classify("x = 1\nprintln(x)\n"), ProgramComplexity::Simple);
        assert_eq!(classify("println(ARGS)"), ProgramComplexity::Simple);
    }

    #[test]
    fn multiple_statements_is_simple() {
        assert_eq!(
            classify("println(\"a\")\nprintln(\"b\")\n"),
            ProgramComplexity::Simple
        );
    }

    #[test]
    fn custom_struct_is_full_pipeline() {
        assert_eq!(
            classify("struct Point10127\n    x::Int\nend\nprintln(1)\n"),
            ProgramComplexity::FullPipeline
        );
    }

    #[test]
    fn custom_function_is_full_pipeline() {
        assert_eq!(
            classify("f10127(x) = x + 1\nprintln(1)\n"),
            ProgramComplexity::FullPipeline
        );
    }

    #[test]
    fn using_statement_is_full_pipeline() {
        assert_eq!(
            classify("using Test\nprintln(1)\n"),
            ProgramComplexity::FullPipeline
        );
    }
}

/// Collect every current-source type's evaluation position (Issues #11025/#11117).
///
/// A module's types are registered under both their qualified and bare names,
/// mirroring how `build_struct_tables` registers them, so a probe consulting this
/// map resolves the same spelling the annotation used.
fn collect_current_type_definition_positions(
    all_structs: &[StructOriginEntry<'_>],
    user_modules: &[crate::ir::core::Module],
    program: &crate::ir::core::Program,
    precompiled_base: Option<&CompiledProgram>,
    repl_current_struct_count: Option<usize>,
    current_input_type_names: Option<&HashSet<String>>,
    positions: &mut HashMap<String, TypeDefinitionPosition>,
) {
    fn record(
        name: &str,
        position: TypeDefinitionPosition,
        positions: &mut HashMap<String, TypeDefinitionPosition>,
    ) {
        // Keep the EARLIEST definition: a later same-named registration (e.g. a
        // module struct's bare-name alias) must not make an earlier type look
        // forward-declared.
        positions
            .entry(name.to_string())
            .and_modify(|existing| {
                if position.is_before(existing.definition_order, existing.source_start) {
                    *existing = position;
                }
            })
            .or_insert(position);
    }
    fn walk_module(
        module: &crate::ir::core::Module,
        parent_path: &str,
        current_input_type_names: Option<&HashSet<String>>,
        positions: &mut HashMap<String, TypeDefinitionPosition>,
    ) {
        if module.is_base_origin || module.is_package_origin {
            return;
        }
        let module_path = if parent_path.is_empty() {
            module.name.clone()
        } else {
            format!("{parent_path}.{}", module.name)
        };
        for def in &module.abstract_types {
            let qualified = format!("{module_path}.{}", def.name);
            if current_input_type_names.is_some_and(|names| !names.contains(&qualified)) {
                continue;
            }
            let position = TypeDefinitionPosition {
                definition_order: def.span.definition_order,
                source_start: def.span.start,
            };
            record(&def.name, position, positions);
            record(&qualified, position, positions);
        }
        for def in &module.primitive_types {
            let qualified = format!("{module_path}.{}", def.name);
            if current_input_type_names.is_some_and(|names| !names.contains(&qualified)) {
                continue;
            }
            let position = TypeDefinitionPosition {
                definition_order: def.span.definition_order,
                source_start: def.span.start,
            };
            record(&def.name, position, positions);
            record(&qualified, position, positions);
        }
        for nested in &module.submodules {
            walk_module(nested, &module_path, current_input_type_names, positions);
        }
    }

    let mut inherited_structs = HashSet::new();
    if let Some(base) = precompiled_base {
        inherited_structs.extend(
            base.struct_defs
                .iter()
                .map(|def| crate::types::nominal_family_name(&def.name).to_string()),
        );
    }
    if program.base_function_count > 0 {
        if let Some(prelude) = crate::get_prelude_program() {
            inherited_structs.extend(prelude.structs.iter().map(|def| def.name.clone()));
        }
    }

    // `build_struct_tables` already computed provenance for concrete types.
    // Its third tuple field is true for Base/stdlib/cache-origin definitions;
    // only false entries share this input's definition-order coordinate space.
    let mut remaining_current_root_structs = repl_current_struct_count;
    let inherited_module_roots: Vec<&str> = user_modules
        .iter()
        .filter(|module| module.is_base_origin || module.is_package_origin)
        .map(|module| module.name.as_str())
        .collect();
    for (def, module_path, inherited) in all_structs {
        if *inherited || (module_path.is_none() && inherited_structs.contains(&def.name)) {
            continue;
        }
        if module_path.as_deref().is_some_and(|path| {
            inherited_module_roots.iter().any(|root| {
                path.strip_prefix(root)
                    .is_some_and(|suffix| suffix.is_empty() || suffix.starts_with('.'))
            })
        }) {
            continue;
        }
        let qualified_name = module_path.as_ref().map_or_else(
            || def.name.clone(),
            |module| format!("{module}.{}", def.name),
        );
        if current_input_type_names.is_some_and(|names| !names.contains(&qualified_name)) {
            continue;
        }
        if module_path.is_none() {
            if let Some(remaining) = remaining_current_root_structs.as_mut() {
                if *remaining == 0 {
                    continue;
                }
                *remaining -= 1;
            }
        }
        let position = TypeDefinitionPosition {
            definition_order: def.span.definition_order,
            source_start: def.span.start,
        };
        record(&def.name, position, positions);
        if let Some(module_path) = module_path {
            record(&format!("{module_path}.{}", def.name), position, positions);
        }
    }

    // Top-level abstract/primitive definitions do not carry the concrete
    // struct provenance bit. Exclude names supplied by the precompiled prefix
    // (or the built-in prelude when compiling without a cache): those types are
    // already visible, and their lowering ordinals are unrelated to this input.
    let mut inherited_abstracts = HashSet::new();
    let mut inherited_primitives = HashSet::new();
    if let Some(base) = precompiled_base {
        inherited_abstracts.extend(base.abstract_types.iter().map(|def| def.name.clone()));
        inherited_primitives.extend(base.primitive_types.iter().map(|def| def.name.clone()));
    }
    if program.base_function_count > 0 {
        if let Some(prelude) = crate::get_prelude_program() {
            inherited_abstracts.extend(prelude.abstract_types.iter().map(|def| def.name.clone()));
            inherited_primitives.extend(prelude.primitive_types.iter().map(|def| def.name.clone()));
        }
    }
    for def in &program.abstract_types {
        if !inherited_abstracts.contains(&def.name)
            && current_input_type_names.is_none_or(|names| names.contains(&def.name))
        {
            record(
                &def.name,
                TypeDefinitionPosition {
                    definition_order: def.span.definition_order,
                    source_start: def.span.start,
                },
                positions,
            );
        }
    }
    for def in &program.primitive_types {
        if !inherited_primitives.contains(&def.name)
            && current_input_type_names.is_none_or(|names| names.contains(&def.name))
        {
            record(
                &def.name,
                TypeDefinitionPosition {
                    definition_order: def.span.definition_order,
                    source_start: def.span.start,
                },
                positions,
            );
        }
    }
    let mut inherited_enums = HashSet::new();
    if let Some(base) = precompiled_base {
        inherited_enums.extend(
            base.enum_defs
                .iter()
                .map(|definition| definition.name.clone()),
        );
    }
    for statement in &program.main.stmts {
        if let Stmt::EnumDef { enum_def, span, .. } = statement {
            if !inherited_enums.contains(&enum_def.name)
                && current_input_type_names.is_none_or(|names| names.contains(&enum_def.name))
            {
                record(
                    &enum_def.name,
                    TypeDefinitionPosition {
                        definition_order: span.definition_order,
                        source_start: span.start,
                    },
                    positions,
                );
            }
        }
    }
    for module in user_modules {
        walk_module(module, "", current_input_type_names, positions);
    }
}

#[cfg(test)]
mod lowering_helper_provenance_11685_tests {
    use super::*;

    #[test]
    fn nested_source_predicate_keeps_runtime_generator_environment_11685() -> Result<(), String> {
        let program = crate::pipeline::parse_and_lower(
            r#"
function capture_probe_11685(a)
    p(x) = x > 1
    collect(x + a for x in 1:3 if p(x))
end
capture_probe_11685(10)
"#,
        )
        .map_err(|error| format!("parse/lower capture probe: {error:?}"))?;

        let output = compile_core_program_internal(
            &program,
            &HashMap::new(),
            &HashMap::new(),
            Default::default(),
        )
        .map_err(|error| format!("compile capture probe: {error:?}"))?;
        let nested_source = output
            .compiled
            .functions
            .iter()
            .find(|function| function.name == "capture_probe_11685#p")
            .ok_or_else(|| "missing compiled nested source predicate".to_string())?;
        assert!(
            !nested_source.is_lowering_helper,
            "an unstamped source-level nested method must not be classified as a lowering helper"
        );
        let lowering_helpers: Vec<_> = output
            .compiled
            .functions
            .iter()
            .filter(|function| function.name.contains("#__gen_"))
            .collect();
        assert_eq!(lowering_helpers.len(), 2);
        assert!(
            lowering_helpers
                .iter()
                .all(|function| function.is_lowering_helper),
            "synthetic generator callables need explicit helper provenance"
        );
        let parent = output
            .compiled
            .functions
            .iter()
            .find(|function| function.name == "capture_probe_11685")
            .ok_or_else(|| "missing compiled parent".to_string())?;
        let body = &output.compiled.code[parent.code_start..parent.code_end];
        assert!(
            body.iter()
                .any(|instr| matches!(instr, Instr::MakeGeneratorRuntimeFiltered(_))),
            "a capture-bearing predicate must use the runtime callable path; body={body:?}"
        );
        Ok(())
    }
}

#[cfg(test)]
#[allow(clippy::expect_used)]
mod synthetic_default_constructor_method_tests {
    use super::*;

    fn compile_source(source: &str) -> CoreCompileOutput {
        let program = crate::pipeline::parse_and_lower(source).expect("parse/lower test source");
        compile_core_program_internal(
            &program,
            &HashMap::new(),
            &HashMap::new(),
            Default::default(),
        )
        .expect("compile test source")
    }

    fn lowered_struct(source: &str, name: &str) -> crate::ir::core::StructDef {
        let program = crate::pipeline::parse_and_lower(source).expect("parse/lower test source");
        program
            .structs
            .iter()
            .find(|struct_def| struct_def.name == name)
            .cloned()
            .unwrap_or_else(|| panic!("missing lowered struct {name}"))
    }

    fn lowered_module_struct(
        source: &str,
        module_name: &str,
        struct_name: &str,
    ) -> crate::ir::core::StructDef {
        let program = crate::pipeline::parse_and_lower(source).expect("parse/lower test source");
        program
            .modules
            .iter()
            .find(|module| module.name == module_name)
            .and_then(|module| {
                module
                    .structs
                    .iter()
                    .find(|struct_def| struct_def.name == struct_name)
            })
            .cloned()
            .unwrap_or_else(|| panic!("missing lowered struct {module_name}.{struct_name}"))
    }

    fn synthetic_defaults(
        struct_def: &crate::ir::core::StructDef,
    ) -> CResult<Vec<SyntheticConstructorMethod>> {
        synthetic_default_constructors(struct_def, None, &HashMap::new())
    }

    #[test]
    fn base_origin_implicit_constructor_stays_on_legacy_route_11062() {
        let mut lowered = lowered_struct(
            r#"
struct BaseImplicit11062{T, N}
    value::T
end
"#,
            "BaseImplicit11062",
        );
        lowered.is_base_origin = true;

        assert!(
            synthetic_defaults(&lowered)
                .expect("inspect Base-origin synthetic defaults")
                .is_empty(),
            "Base-origin implicit constructors must remain on the cache-safe legacy route"
        );
    }

    #[test]
    fn base_origin_same_arity_outer_keeps_raw_allocation_11062() {
        let mut program = crate::pipeline::parse_and_lower(
            r#"
struct BaseOuter11062
    value::Int64
end

BaseOuter11062(value::Integer) = BaseOuter11062(Int64(value))
println(BaseOuter11062(7).value)
"#,
        )
        .expect("parse/lower Base-origin constructor probe");
        program.mark_structs_as_base_origin();
        let output = compile_core_program_internal(
            &program,
            &HashMap::new(),
            &HashMap::new(),
            Default::default(),
        )
        .expect("compile Base-origin constructor probe");

        assert_eq!(
            crate::test_runtime::run_compiled_program(output.compiled, 1)
                .expect("same-arity Base outer must terminate through raw allocation"),
            "7\n"
        );
    }

    #[test]
    fn synthetic_parametric_outer_registers_inferred_instantiation_11147_4269() {
        let output = compile_source(
            r#"
struct InferredDefault11147{T}
    value::T
    label
end

make_inferred_default11147() = InferredDefault11147(7, "x")
make_inferred_default11147()
"#,
        );
        let compile_context = output
            .compiled
            .compile_context
            .as_ref()
            .expect("fresh compile context");

        assert!(
            compile_context
                .struct_table
                .contains_key("InferredDefault11147{Int64}"),
            "synthetic default outer dispatch must retain the inferred concrete instantiation"
        );
    }

    #[test]
    fn synthetic_default_constructor_method_rows_11147() {
        let output = compile_source(
            r#"
struct Plain11147
    x::Int64
end

struct Box11147{T}
    x::T
    n::Int64
end
"#,
        );

        let plain_table = output.method_tables.get("Plain11147");
        assert!(
            plain_table.is_some(),
            "Plain11147 must have a synthetic default-constructor method table"
        );
        let plain_table = plain_table.expect("asserted above");
        let plain_inner_rows: Vec<_> = plain_table
            .methods
            .iter()
            .filter(|method| {
                plain_table.constructor_self_family(method.global_index)
                    == Some(ConstructorSelfFamily::BareInner)
            })
            .collect();
        let plain_outer_rows: Vec<_> = plain_table
            .methods
            .iter()
            .filter(|method| {
                plain_table
                    .constructor_self_family(method.global_index)
                    .is_none()
            })
            .collect();
        assert_eq!(
            plain_table.methods.len(),
            2,
            "Plain11147 must contain only its synthetic BareInner and ordinary outer rows"
        );
        assert_eq!(
            plain_inner_rows.len(),
            1,
            "Plain11147 must have exactly one BareInner synthetic constructor row"
        );
        assert_eq!(
            plain_outer_rows.len(),
            1,
            "Plain11147 must have exactly one ordinary synthetic outer row"
        );
        let plain_inner = plain_inner_rows[0];
        assert_eq!(
            plain_inner.projected_param_julia_types(),
            vec![JuliaType::Any],
            "Plain11147 synthetic BareInner parameters"
        );
        let plain_outer = plain_outer_rows[0];
        assert_eq!(
            plain_table.constructor_self_family(plain_outer.global_index),
            None,
            "Plain11147 ordinary outer row must not carry inner-constructor origin"
        );
        assert!(
            plain_table
                .is_synthetic_default_outer_for_owner(plain_outer.global_index, "Plain11147"),
            "Plain11147 typed default outer must retain transient compile provenance"
        );
        assert_eq!(
            plain_outer.projected_param_julia_types(),
            vec![JuliaType::Int64],
            "Plain11147 ordinary synthetic outer parameters"
        );

        let box_table = output
            .method_tables
            .get("Box11147")
            .expect("Box11147 must have a synthetic default-constructor method table");
        let explicit_inner_rows: Vec<_> = box_table
            .methods
            .iter()
            .filter(|method| {
                box_table.constructor_self_family(method.global_index)
                    == Some(ConstructorSelfFamily::ExplicitParametricInner)
            })
            .collect();
        let ordinary_outer_rows: Vec<_> = box_table
            .methods
            .iter()
            .filter(|method| {
                box_table
                    .constructor_self_family(method.global_index)
                    .is_none()
            })
            .collect();
        assert_eq!(
            box_table.methods.len(),
            2,
            "Box11147 must contain only its synthetic explicit inner and ordinary outer rows"
        );
        assert_eq!(
            explicit_inner_rows.len(),
            1,
            "Box11147 must have exactly one ExplicitParametricInner synthetic row"
        );
        assert_eq!(
            ordinary_outer_rows.len(),
            1,
            "Box11147 must have exactly one ordinary synthetic outer row"
        );

        let explicit_inner = explicit_inner_rows[0];
        assert_eq!(
            explicit_inner.projected_param_julia_types(),
            vec![JuliaType::Any, JuliaType::Any],
            "Box11147 explicit synthetic inner parameters"
        );
        assert_eq!(
            explicit_inner.explicit_constructor_type_name.as_deref(),
            Some("Box11147"),
            "Box11147 explicit synthetic inner self type name"
        );
        assert_eq!(
            explicit_inner.explicit_constructor_type_arguments,
            vec![TypeExpr::TypeVar("T".to_string())],
            "Box11147 explicit synthetic inner self type arguments"
        );
        assert_eq!(
            explicit_inner.explicit_constructor_type_parameter_names,
            vec!["T".to_string()],
            "Box11147 explicit synthetic inner self binder names"
        );

        let ordinary_outer = ordinary_outer_rows[0];
        assert_eq!(
            box_table.constructor_self_family(ordinary_outer.global_index),
            None,
            "Box11147 ordinary outer row must not carry inner-constructor origin"
        );
        assert!(
            box_table.is_synthetic_default_outer_for_owner(ordinary_outer.global_index, "Box11147"),
            "Box11147 typed default outer must retain transient compile provenance"
        );
        assert_eq!(
            ordinary_outer.projected_param_julia_types(),
            vec![JuliaType::TypeVar("T".to_string(), None), JuliaType::Int64],
            "Box11147 ordinary synthetic outer parameters"
        );
    }

    #[test]
    fn nonparametric_type_id_zero_raw_constructor_stays_concrete_11147_11062() {
        let lowered = lowered_struct(
            r#"
abstract type Exception end
struct ErrorException <: Exception
    msg::AbstractString
end
"#,
            "ErrorException",
        );
        assert!(
            lowered.type_params.is_empty(),
            "ErrorException has no declared type parameters"
        );

        let output = compile_source(
            r#"
make_error_exception_11147() = ErrorException("x")
typeof(make_error_exception_11147())
"#,
        );
        let type_id = output
            .compiled
            .struct_defs
            .iter()
            .position(|def| def.name == "ErrorException")
            .expect("compiled ErrorException definition");
        assert_eq!(
            type_id, 0,
            "the first concrete Base struct legitimately owns id zero"
        );
        let compile_context = output
            .compiled
            .compile_context
            .as_ref()
            .expect("fresh compile context");
        assert!(
            !compile_context
                .parametric_structs
                .contains_key("ErrorException"),
            "ErrorException must not be registered as parametric"
        );
        assert_eq!(
            resolve_inner_constructor_target(
                &lowered,
                "ErrorException",
                &compile_context.struct_table,
                &compile_context.parametric_structs,
            )
            .expect("registered concrete constructor target"),
            InnerCtorTarget::Concrete { type_id: 0 },
            "a legitimate concrete id zero must remain structurally distinct from parametric"
        );

        let wrapper = output
            .compiled
            .functions
            .iter()
            .find(|function| function.name == "make_error_exception_11147")
            .expect("id-zero constructor wrapper");
        let code = &output.compiled.code[wrapper.code_start..wrapper.code_end];
        assert!(
            code.iter()
                .any(|instr| matches!(instr, Instr::NewStruct(id, 1) if *id == type_id)),
            "a nonparametric id-zero constructor must allocate concretely: {code:?}"
        );
        assert!(
            !code
                .iter()
                .any(|instr| matches!(instr, Instr::NewParametricStruct(_, _))),
            "a nonparametric id-zero constructor must not enter parametric allocation: {code:?}"
        );
    }

    #[test]
    fn top_level_inner_constructor_keeps_main_owner_under_module_leaf_collision_10445() {
        let output = compile_source(
            r#"
module OwnerCollision10445
struct ParseError10445
    source::Int64
    diagnostics::Int64
    incomplete_tag::Int64
end
end

struct ParseError10445
    msg::String
    detail
    ParseError10445(msg::String, detail) = new(msg, detail)
end
"#,
        );

        let compile_context = output.compiled.compile_context.as_ref();
        assert!(
            compile_context.is_some(),
            "fresh compile context must be present"
        );
        let Some(compile_context) = compile_context else {
            return;
        };
        let main_entry = compile_context
            .struct_table
            .resolve_in_owner("Main", "ParseError10445");
        assert!(
            main_entry.is_some(),
            "top-level declaration must be registered"
        );
        let Some((_, main_info)) = main_entry else {
            return;
        };
        let main_type_id = main_info.type_id;
        let module_entry = compile_context
            .struct_table
            .resolve_in_owner("OwnerCollision10445", "ParseError10445");
        assert!(
            module_entry.is_some(),
            "module declaration must be registered"
        );
        let Some((_, module_info)) = module_entry else {
            return;
        };
        let module_type_id = module_info.type_id;
        assert_ne!(main_type_id, module_type_id);

        let function = output
            .compiled
            .functions
            .iter()
            .find(|function| function.name == "ParseError10445" && function.params.len() == 2);
        assert!(
            function.is_some(),
            "top-level explicit inner constructor must exist"
        );
        let Some(function) = function else {
            return;
        };
        let code = &output.compiled.code[function.code_start..function.code_end];
        assert!(
            code.iter()
                .any(|instr| matches!(instr, Instr::NewStruct(id, 2) if *id == main_type_id)),
            "top-level inner must allocate its Main-owned declaration: {code:?}"
        );
        assert!(
            !code
                .iter()
                .any(|instr| matches!(instr, Instr::NewStruct(id, 2) if *id == module_type_id)),
            "module-owned same-leaf alias must not hijack the top-level inner: {code:?}"
        );
    }

    #[test]
    fn module_parametric_inner_uses_owner_exact_registration_11147_10342() {
        let output = compile_source(
            r#"
struct Sentinel11147
    x::Int64
end

struct OwnerCollision11147
    x::Int64
end

module ExactOwner11147
struct OwnerCollision11147{T}
    x::T
    OwnerCollision11147{T}(x) where {T} = new{T}(x)
end

struct ParametricControl11147{T}
    x::T
    ParametricControl11147{T}(x) where {T} = new{T}(x)
end
end
"#,
        );

        fn explicit_inner_code<'a>(output: &'a CoreCompileOutput, table_name: &str) -> &'a [Instr] {
            let table = output
                .method_tables
                .get(table_name)
                .unwrap_or_else(|| panic!("missing qualified constructor table {table_name}"));
            let global_index = table
                .methods
                .iter()
                .find(|method| {
                    table.constructor_self_family(method.global_index)
                        == Some(ConstructorSelfFamily::ExplicitParametricInner)
                })
                .unwrap_or_else(|| panic!("missing explicit inner row for {table_name}"))
                .global_index;
            let function = &output.compiled.functions[global_index];
            &output.compiled.code[function.code_start..function.code_end]
        }

        let compile_context = output
            .compiled
            .compile_context
            .as_ref()
            .expect("fresh compile context");
        let unrelated_concrete_id = compile_context
            .struct_table
            .get("OwnerCollision11147")
            .expect("top-level concrete collision entry")
            .type_id;
        assert_ne!(
            unrelated_concrete_id, 0,
            "the owner collision must be independent of the id-zero regression"
        );
        assert!(
            compile_context
                .parametric_structs
                .contains_key("ExactOwner11147.OwnerCollision11147"),
            "the module-owned declaration is authoritatively parametric"
        );

        let control_code = explicit_inner_code(&output, "ExactOwner11147.ParametricControl11147");
        assert!(
            control_code.iter().any(|instr| matches!(
                instr,
                Instr::NewDynamicParametricStruct(base, 1, 1)
                    if base == "ExactOwner11147.ParametricControl11147"
            )),
            "an unshadowed parametric inner keeps its qualified owner: {control_code:?}"
        );
        assert!(
            !control_code
                .iter()
                .any(|instr| matches!(instr, Instr::NewStruct(_, _))),
            "the parametric negative control must not allocate concretely: {control_code:?}"
        );

        let collision_code = explicit_inner_code(&output, "ExactOwner11147.OwnerCollision11147");
        assert!(
            !collision_code.iter().any(
                |instr| matches!(instr, Instr::NewStruct(id, 1) if *id == unrelated_concrete_id)
            ),
            "the module-owned parametric inner must not inherit the unrelated top-level concrete id {unrelated_concrete_id}: {collision_code:?}"
        );
        assert!(
            collision_code.iter().any(|instr| matches!(
                instr,
                Instr::NewDynamicParametricStruct(base, 1, 1)
                    if base == "ExactOwner11147.OwnerCollision11147"
            )),
            "the module-owned parametric inner must allocate through its exact owner: {collision_code:?}"
        );
    }

    #[test]
    fn sibling_parametric_inner_allocations_keep_qualified_owner_11147_10342() {
        let output = compile_source(
            r#"
module SiblingA11147
struct Box11147{T}
    value::T
    Box11147{T}(value) where {T} = new{T}(value)
end
end

module SiblingB11147
struct Box11147{T}
    value::T
end
end
"#,
        );

        fn explicit_inner<'a>(
            output: &'a CoreCompileOutput,
            qualified_name: &str,
        ) -> (usize, &'a [Instr]) {
            let table = output
                .method_tables
                .get(qualified_name)
                .unwrap_or_else(|| panic!("missing qualified constructor table {qualified_name}"));
            let global_index = table
                .methods
                .iter()
                .find(|method| {
                    table.constructor_self_family(method.global_index)
                        == Some(ConstructorSelfFamily::ExplicitParametricInner)
                })
                .unwrap_or_else(|| panic!("missing explicit inner row for {qualified_name}"))
                .global_index;
            let function = &output.compiled.functions[global_index];
            (
                global_index,
                &output.compiled.code[function.code_start..function.code_end],
            )
        }

        let (a_index, a_code) = explicit_inner(&output, "SiblingA11147.Box11147");
        let (b_index, b_code) = explicit_inner(&output, "SiblingB11147.Box11147");
        assert_ne!(
            a_index, b_index,
            "sibling owners must select distinct exact-qualified constructor methods"
        );

        for (qualified_name, code) in [
            ("SiblingA11147.Box11147", a_code),
            ("SiblingB11147.Box11147", b_code),
        ] {
            assert!(
                code.iter().any(|instr| matches!(
                    instr,
                    Instr::NewDynamicParametricStruct(base, 1, 1)
                        if base == qualified_name
                )),
                "{qualified_name} must allocate through its exact qualified owner: {code:?}"
            );
            assert!(
                !code.iter().any(|instr| matches!(
                    instr,
                    Instr::NewDynamicParametricStruct(base, _, _)
                        if base == "Box11147"
                )),
                "{qualified_name} must not collapse to the shared bare leaf: {code:?}"
            );
        }
    }

    #[test]
    fn missing_owner_exact_inner_constructor_target_is_compile_error_11147_10342() {
        let source = r#"
module MissingOwner11147
struct MissingConcrete11147
    x::Int64
end

struct MissingParametric11147{T}
    x::T
end
end
"#;
        let concrete = lowered_module_struct(source, "MissingOwner11147", "MissingConcrete11147");
        let parametric =
            lowered_module_struct(source, "MissingOwner11147", "MissingParametric11147");
        assert!(!concrete.is_parametric());
        assert!(parametric.is_parametric());

        let struct_table = StructRegistry::new();
        let parametric_structs = HashMap::new();

        let concrete_result = resolve_inner_constructor_target(
            &concrete,
            "MissingOwner11147.MissingConcrete11147",
            &struct_table,
            &parametric_structs,
        );
        let parametric_result = resolve_inner_constructor_target(
            &parametric,
            "MissingOwner11147.MissingParametric11147",
            &struct_table,
            &parametric_structs,
        );
        assert!(
            concrete_result.is_err() && parametric_result.is_err(),
            "both missing owner-exact targets must fail closed: concrete={concrete_result:?}, parametric={parametric_result:?}"
        );
        let (Err(concrete_error), Err(parametric_error)) = (concrete_result, parametric_result)
        else {
            panic!("asserted both missing targets are errors")
        };
        assert_eq!(
            concrete_error.to_string(),
            format!(
                "internal: missing concrete inner-constructor target `MissingOwner11147.MissingConcrete11147` at line {}, column {}",
                concrete.span.start_line, concrete.span.start_column
            )
        );

        assert_eq!(
            parametric_error.to_string(),
            format!(
                "internal: missing parametric inner-constructor target `MissingOwner11147.MissingParametric11147` at line {}, column {}",
                parametric.span.start_line, parametric.span.start_column
            )
        );

        let mut registered_struct_table = StructRegistry::new();
        registered_struct_table.insert(
            "MissingOwner11147.MissingConcrete11147",
            StructInfo {
                type_id: 17,
                is_mutable: false,
                fields: vec![],
                has_inner_constructor: false,
            },
        );
        assert_eq!(
            resolve_inner_constructor_target(
                &concrete,
                "MissingOwner11147.MissingConcrete11147",
                &registered_struct_table,
                &HashMap::new(),
            )
            .expect("registered concrete owner target"),
            InnerCtorTarget::Concrete { type_id: 17 },
            "a registered concrete owner returns only its exact type id"
        );

        let mut registered_parametric_structs = HashMap::new();
        registered_parametric_structs.insert(
            "MissingOwner11147.MissingParametric11147".to_string(),
            ParametricStructDef {
                def: parametric.clone(),
            },
        );
        assert_eq!(
            resolve_inner_constructor_target(
                &parametric,
                "MissingOwner11147.MissingParametric11147",
                &StructRegistry::new(),
                &registered_parametric_structs,
            )
            .expect("registered parametric owner target"),
            InnerCtorTarget::Parametric {
                qualified_name: "MissingOwner11147.MissingParametric11147".to_string(),
            },
            "a registered parametric owner returns only its exact qualified name"
        );
    }

    #[test]
    fn user_inner_suppresses_synthetic_defaults_11147() {
        let output = compile_source(
            r#"
struct UserInner11147{T}
    x::T
    n::Int64
    UserInner11147{T}(x::T, n::Int64) where {T} = new{T}(x, n)
end
"#,
        );

        let table = output
            .method_tables
            .get("UserInner11147")
            .expect("source-written inner constructor must have a method table");
        let inner_rows: Vec<_> = table
            .methods
            .iter()
            .filter(|method| table.is_inner_constructor(method.global_index))
            .collect();
        assert_eq!(inner_rows.len(), 1, "only the source-written inner remains");
        assert_eq!(table.methods.len(), 1, "no synthetic outer/default row");

        let inner = inner_rows[0];
        assert_eq!(
            table.constructor_self_family(inner.global_index),
            Some(ConstructorSelfFamily::ExplicitParametricInner),
            "source-written UserInner11147 row origin"
        );
        assert_eq!(
            inner.projected_param_julia_types(),
            vec![JuliaType::TypeVar("T".to_string(), None), JuliaType::Int64,],
            "source-written UserInner11147 parameters"
        );
        assert!(
            !table
                .methods
                .iter()
                .any(|method| { table.constructor_self_family(method.global_index).is_none() }),
            "a user inner suppresses the ordinary synthetic outer"
        );
        assert!(
            !table.methods.iter().any(|method| {
                method.projected_param_julia_types() == vec![JuliaType::Any, JuliaType::Any]
            }),
            "a user inner suppresses the Any-typed synthetic inner"
        );
    }

    #[test]
    fn synthetic_default_constructor_generation_shape_11147() {
        let all_any = lowered_struct(
            r#"
struct AllAny11147
    x::Any
    y
end
"#,
            "AllAny11147",
        );
        let all_any_methods = synthetic_defaults(&all_any).expect("generate AllAny defaults");
        assert_eq!(all_any_methods.len(), 1, "all-Any keeps only its outer");
        assert_eq!(
            all_any_methods[0].kind,
            SyntheticConstructorKind::DefaultOuter
        );
        assert!(matches!(
            all_any_methods[0].ctor.body.stmts.as_slice(),
            [Stmt::Return {
                value: Some(Expr::New { args, .. }),
                ..
            }] if args.len() == 2
        ));

        let empty = lowered_struct("struct Empty11147 end", "Empty11147");
        let empty_methods = synthetic_defaults(&empty).expect("generate zero-field defaults");
        assert_eq!(empty_methods.len(), 1, "zero-field keeps only its outer");
        assert_eq!(
            empty_methods[0].kind,
            SyntheticConstructorKind::DefaultOuter
        );

        let phantom = lowered_struct(
            r#"
struct Phantom11147{T}
    x::Int64
end
"#,
            "Phantom11147",
        );
        let phantom_methods = synthetic_defaults(&phantom).expect("generate phantom defaults");
        assert_eq!(phantom_methods.len(), 1, "phantom type omits bare outer");
        assert_eq!(
            phantom_methods[0].kind,
            SyntheticConstructorKind::DefaultInner(ConstructorSelfFamily::ExplicitParametricInner)
        );

        let nested = lowered_struct(
            r#"
struct Nested11147{T}
    values::Vector{T}
    metadata::Any
end
"#,
            "Nested11147",
        );
        let nested_methods = synthetic_defaults(&nested).expect("generate nested defaults");
        assert_eq!(
            nested_methods.len(),
            2,
            "inferable parametric gets two rows"
        );
        let inner = nested_methods
            .iter()
            .find(|method| {
                method.kind
                    == SyntheticConstructorKind::DefaultInner(
                        ConstructorSelfFamily::ExplicitParametricInner,
                    )
            })
            .expect("nested explicit inner");
        let [Stmt::Return {
            value: Some(Expr::New { args, .. }),
            ..
        }] = inner.ctor.body.stmts.as_slice()
        else {
            panic!("synthetic inner must end in one transactional new")
        };
        let Expr::LetBlock { bindings, body, .. } = &args[0] else {
            panic!("typed nested field must use a guarded conversion let")
        };
        assert!(matches!(
            &bindings[0].1,
            Expr::DynamicTypeConstruct {
                base,
                type_args,
                ..
            } if base.as_ref() == "Vector"
                && matches!(type_args.as_slice(), [Expr::Var(name, _)] if name.as_ref() == "T")
        ));
        assert!(matches!(
            body.stmts.as_slice(),
            [Stmt::Expr {
                expr: Expr::Ternary { .. },
                ..
            }]
        ));
        assert!(matches!(&args[1], Expr::Var(_, _)), "Any bypasses convert");
    }

    #[test]
    fn synthetic_default_constructor_infers_through_typevar_bound_11147() {
        let bounded = lowered_struct(
            r#"
struct BoundInfer11147{S,T<:AbstractArray{S}}
    x::T
end
"#,
            "BoundInfer11147",
        );
        let methods = synthetic_defaults(&bounded).expect("generate bound-infer defaults");
        assert!(
            methods
                .iter()
                .any(|method| method.kind == SyntheticConstructorKind::DefaultOuter),
            "T is field-inferable and its upper bound transitively constrains S"
        );
    }

    #[test]
    fn synthetic_default_constructor_qualifies_module_local_field_target_11147() {
        let output = compile_source(
            r#"
module Owner11147
struct Box{T}
    value::T
end

struct Holder{T}
    box::Box{T}
    values::Vector{T}
end
end

module Other11147
struct Box{T}
    value::T
end
end
"#,
        );

        let table = output
            .method_tables
            .get("Owner11147.Holder")
            .expect("qualified synthetic constructor table");
        let inner_index = table
            .methods
            .iter()
            .find(|method| {
                table.constructor_self_family(method.global_index)
                    == Some(ConstructorSelfFamily::ExplicitParametricInner)
            })
            .expect("qualified synthetic explicit inner")
            .global_index;
        let inner = &output.compiled.functions[inner_index];
        let code = &output.compiled.code[inner.code_start..inner.code_end];

        assert!(
            code.iter().any(|instr| {
                matches!(
                    instr,
                    Instr::ConstructParametricType(base, 1) if base == "Owner11147.Box"
                )
            }),
            "module-local field target must retain its lexical owner: {code:?}"
        );
        assert!(
            code.iter()
                .any(|instr| matches!(instr, Instr::ApplyTypeDynamic(1))),
            "non-local Base.Vector must resolve through the lexical binding: {code:?}"
        );
        assert!(
            !code.iter().any(|instr| {
                matches!(
                    instr,
                    Instr::ConstructParametricType(base, 1)
                        if base == "Box" || base == "Other11147.Box" || base == "Owner11147.Vector"
                )
            }),
            "synthetic target must not use an ambiguous or invented owner: {code:?}"
        );
    }

    #[test]
    fn synthetic_default_constructor_resolves_imported_field_target_11147() {
        let output = compile_source(
            r#"
module Source11147
export ImportedBox
struct ImportedBox{T}
    value::T
end
end

module Wrong11147
struct ImportedBox{T}
    value::T
end
end

module ImportOwner11147
using ..Source11147: ImportedBox
struct Holder{T}
    box::ImportedBox{T}
    values::Vector{T}
end
end

box = Source11147.ImportedBox{Int64}(7)
holder = ImportOwner11147.Holder{Int64}(box, [1])
println(holder.values isa Vector{Int64})
println(holder.box isa Source11147.ImportedBox{Int64})
println(holder.box.value)
"#,
        );

        let table = output
            .method_tables
            .get("ImportOwner11147.Holder")
            .expect("import-owner synthetic constructor table");
        let inner_index = table
            .methods
            .iter()
            .find(|method| {
                table.constructor_self_family(method.global_index)
                    == Some(ConstructorSelfFamily::ExplicitParametricInner)
            })
            .expect("import-owner synthetic explicit inner")
            .global_index;
        let inner = &output.compiled.functions[inner_index];
        let code = &output.compiled.code[inner.code_start..inner.code_end];

        assert!(
            code.iter()
                .filter(|instr| matches!(instr, Instr::ApplyTypeDynamic(1)))
                .count()
                >= 2,
            "imported field target and Base.Vector must resolve through lexical bindings: {code:?}"
        );
        assert!(
            !code.iter().any(|instr| {
                matches!(
                    instr,
                    Instr::ConstructParametricType(base, 1)
                        if base == "ImportedBox"
                            || base == "Wrong11147.ImportedBox"
                            || base == "ImportOwner11147.ImportedBox"
                )
            }),
            "imported field target must not use an ambiguous or invented literal base: {code:?}"
        );
        let import_bindings = &output
            .compiled
            .compile_context
            .as_ref()
            .expect("compiled import-owner context")
            .module_imported_bindings;
        assert_eq!(
            import_bindings.get("ImportOwner11147.ImportedBox"),
            Some(&"Source11147.ImportedBox".to_string()),
            "the dynamic base lookup must retain the selected import owner"
        );

        // Cross-module `isa` cannot currently distinguish the two same-short-
        // name structs (Issue #10342), so ownership is asserted from the
        // compiled binding above and runtime validates the selected value.
        let runtime = crate::test_runtime::run_compiled_program(output.compiled, 1)
            .expect("run imported synthetic-constructor program");
        assert_eq!(runtime, "true\ntrue\n7\n");
    }

    #[test]
    fn synthetic_default_constructor_late_outer_replaces_default_11147() {
        let output = compile_source(
            r#"
struct Replaced11147
    x::Int64
end

Replaced11147(x::Int64) = nothing
println(Replaced11147(1) === nothing)

struct ReplacedAny11147
    x
end

ReplacedAny11147(x) = "source outer"
println(ReplacedAny11147(1) == "source outer")
"#,
        );
        let table = output
            .method_tables
            .get("Replaced11147")
            .expect("constructor table");
        assert_eq!(
            table.methods.len(),
            2,
            "BareInner plus later ordinary outer"
        );
        let ordinary = table
            .methods
            .iter()
            .find(|method| table.constructor_self_family(method.global_index).is_none())
            .expect("later ordinary row");
        assert_eq!(
            output.compiled.functions[ordinary.global_index].def_line, 6,
            "the source-written later outer remains authoritative"
        );
        assert!(
            !table.is_synthetic_default_outer_for_owner(ordinary.global_index, "Replaced11147",),
            "a later source-written outer must not inherit synthetic provenance"
        );
        let runtime = crate::test_runtime::run_compiled_program(output.compiled, 1)
            .expect("run later-outer replacement program");
        assert_eq!(runtime, "true\ntrue\n");
    }

    #[test]
    fn selected_concrete_synthetic_outer_uses_raw_allocation_11147() {
        let output = compile_source(
            r#"
struct RawOuter11147
    x::Int64
end

make_raw_outer11147() = RawOuter11147(1)
make_raw_outer11147()
"#,
        );
        let wrapper = output
            .compiled
            .functions
            .iter()
            .find(|function| function.name == "make_raw_outer11147")
            .expect("raw outer wrapper");
        let wrapper_code = &output.compiled.code[wrapper.code_start..wrapper.code_end];
        assert!(
            wrapper_code
                .iter()
                .any(|instr| matches!(instr, Instr::NewStruct(_, 1))),
            "a statically selected typed synthetic outer is equivalent to raw allocation: {wrapper_code:?}"
        );
    }

    #[test]
    fn parametric_default_outer_does_not_redispatch_to_explicit_self_11147() {
        let output = compile_source(
            r#"
struct DirectOuter11147{T}
    value::T
end

DirectOuter11147{T}(value::T) where {T} = nothing
make_direct_outer11147(value) = DirectOuter11147(value)

bare = DirectOuter11147(1)
wrapped = make_direct_outer11147(2)

println(bare isa DirectOuter11147{Int64})
println(bare.value)
println(wrapped isa DirectOuter11147{Int64})
println(wrapped.value)
"#,
        );

        let runtime = crate::test_runtime::run_compiled_program(output.compiled, 1)
            .expect("run direct parametric default-outer program");
        assert_eq!(runtime, "true\n1\ntrue\n2\n");
    }

    #[test]
    fn synthetic_default_constructor_runtime_any_uses_convert_dispatch_11147() {
        let output = compile_source(
            r#"
struct ConvertSource11147
end

Base.convert(::Type{Int64}, ::ConvertSource11147) = 42

struct Routed11147
    x::Int64
end

const AliasInt11147 = Int64
struct AliasRouted11147
    x::AliasInt11147
end

route_runtime_11147(x) = Routed11147(x)
route_static_11147() = Routed11147(ConvertSource11147())
Base.convert(::Type{Int64}, ::Int64) = 999
route_identity_11147(x) = Routed11147(x)
route_alias_11147(x) = AliasRouted11147(x)

println(route_runtime_11147(ConvertSource11147()).x)
println(route_static_11147().x)
println(route_identity_11147(1).x)
println(route_alias_11147(1).x)

try
    Routed11147(1.5)
    println(false)
catch err
    println(err isa InexactError)
end
"#,
        );
        let wrapper = output
            .compiled
            .functions
            .iter()
            .find(|function| function.name == "route_runtime_11147")
            .expect("runtime-Any wrapper function");
        let wrapper_code = &output.compiled.code[wrapper.code_start..wrapper.code_end];
        assert!(
            !wrapper_code
                .iter()
                .any(|instr| matches!(instr, Instr::DynamicToI64)),
            "runtime Any must not bypass user convert dispatch via primitive coercion: {wrapper_code:?}"
        );
        assert!(
            wrapper_code.iter().any(|instr| matches!(
                instr,
                Instr::CallBuiltin(crate::builtins::BuiltinId::Isa, 2)
            )),
            "the inlined synthetic inner must preserve upstream's isa guard: {wrapper_code:?}"
        );
        assert!(
            wrapper_code.iter().any(|instr| matches!(
                instr,
                Instr::CallBuiltin(crate::builtins::BuiltinId::Convert, 2)
            )),
            "the inlined synthetic inner must dispatch convert at runtime: {wrapper_code:?}"
        );
        assert!(
            wrapper_code
                .iter()
                .any(|instr| matches!(instr, Instr::NewStruct(_, 1))),
            "the inlined synthetic inner must allocate only after conversion: {wrapper_code:?}"
        );
        let alias_wrapper = output
            .compiled
            .functions
            .iter()
            .find(|function| function.name == "route_alias_11147")
            .expect("alias-field wrapper function");
        let alias_wrapper_code =
            &output.compiled.code[alias_wrapper.code_start..alias_wrapper.code_end];
        assert!(
            !alias_wrapper_code
                .iter()
                .any(|instr| matches!(instr, Instr::NewStruct(_, 1))),
            "an unresolved field alias must use the ordinary synthetic method: {alias_wrapper_code:?}"
        );
        let runtime = crate::test_runtime::run_compiled_program(output.compiled, 1)
            .expect("run runtime-Any custom conversion program");
        assert_eq!(runtime, "42\n42\n1\n1\ntrue\n");
    }

    #[test]
    fn runtime_bound_explicit_call_routes_through_synthetic_inner_8103_11147() {
        let output = compile_source(
            r#"
struct RuntimeExplicit8103{N,T}
    values::Vector{T}
end

function RuntimeExplicit8103{N,T}(x::Number) where {N,T}
    RuntimeExplicit8103{N,T}([T(x), T(2 * x)])
end

RuntimeExplicit8103{2,Float64}(5.0)
"#,
        );

        let bare_table = output
            .method_tables
            .get("RuntimeExplicit8103")
            .expect("synthetic constructor table");
        let inner_index = bare_table
            .methods
            .iter()
            .find(|method| {
                bare_table.constructor_self_family(method.global_index)
                    == Some(ConstructorSelfFamily::ExplicitParametricInner)
            })
            .expect("synthetic explicit-parametric inner")
            .global_index;
        let outer_index = output
            .method_tables
            .iter()
            .filter(|(name, _)| name.starts_with("RuntimeExplicit8103{"))
            .flat_map(|(_, table)| table.methods.iter())
            .find(|method| method.accepts_arity(1))
            .expect("source-written explicit outer")
            .global_index;

        let outer = &output.compiled.functions[outer_index];
        let outer_code = &output.compiled.code[outer.code_start..outer.code_end];
        assert!(
            outer_code.iter().any(|instr| {
                matches!(
                    instr,
                    Instr::CallStaticParametric(call)
                        if call.func_index == inner_index
                            && call.forward_caller_type_bindings
                )
            }),
            "runtime-bound direct syntax must call the synthetic explicit inner: {outer_code:?}"
        );
        assert!(
            !outer_code.iter().any(|instr| matches!(
                instr,
                Instr::NewDynamicParametricStruct(base, 1, 2)
                    if base == "RuntimeExplicit8103"
            )),
            "the user outer must not raw-allocate the parametric struct: {outer_code:?}"
        );

        let inner = &output.compiled.functions[inner_index];
        let inner_code = &output.compiled.code[inner.code_start..inner.code_end];
        assert!(
            inner_code.iter().any(|instr| matches!(
                instr,
                Instr::NewDynamicParametricStruct(base, 1, 2)
                    if base == "RuntimeExplicit8103"
            )),
            "the selected synthetic inner must retain the final allocation: {inner_code:?}"
        );

        let main_code = &output.compiled.code[output.compiled.entry..];
        assert!(
            main_code.iter().any(|instr| matches!(
                instr,
                Instr::CallStaticParametric(call) if call.func_index == outer_index
            )),
            "the concrete explicit call must still select the source-written outer: {main_code:?}"
        );
    }
}
