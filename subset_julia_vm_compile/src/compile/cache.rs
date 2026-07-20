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

// Issue #10906 (Phase 1c of #10869): the "cache load" entrypoint's
// deserialize/load boundary — zero real unwrap_used/expect_used sites in
// production code (every match is inside the cfg(test) module, which carries
// an explicit allow). Malformed/tampered cache bytes must surface as a
// `Result` (cache miss), never panic — see `docs/vm/CACHE_ARCHITECTURE.md`.
#![deny(clippy::unwrap_used)]
#![deny(clippy::expect_used)]

use super::abstract_interp::engine::{CachedReturn, InferenceCacheKey};
use super::types::CResult;
use crate::bytecode::{
    CompiledProgram, MethodSig, MethodTable, ReplDefinitionActivation, ReplMethodIdentity,
    RuntimeCompileContext, RuntimeNominalActivation, RuntimeNominalDefInfo, SpecializableFunction,
    StructDefInfo, ValueType,
};
use crate::ir::core::{Block, Expr, Function, Module, Program, Stmt, TypeAliasDef};
use std::cell::RefCell;
use std::collections::hash_map::DefaultHasher;
use std::collections::{BTreeMap, BTreeSet, HashMap, HashSet};
use std::env;
use std::fs;
use std::hash::{Hash, Hasher};
use std::io::ErrorKind;
use std::path::{Path, PathBuf};
use std::rc::Rc;
use std::sync::Arc;
use std::time::{Duration, SystemTime};

/// Check if cache debug logging is enabled via environment variable
fn should_log_cache() -> bool {
    env::var("SUBSET_JULIA_VM_CACHE_DEBUG").is_ok()
}

/// Warm-start prefetch (Issue #6348, phase 2; extended by Issue #10114).
///
/// A one-shot CLI run spends its first ~16 ms on the main thread loading the
/// prelude `Program` and merging it with user code, then ~9 ms deserializing
/// the Base cache inside compile. The Base cache contains VM `Value`
/// constants (`Rc`-based, not `Send`), so it must stay on the compiling
/// thread — instead, `begin_warm_start_prefetch` moves the *prelude side* to
/// a background thread (`Program` is `Send`; the prelude already lives in a
/// sync `Lazy`): it warms the prelude Lazy, clones the Base function IR, AND
/// (Issue #10114) inserts every clone into a `(function_table,
/// ambiguous_functions)` pair via
/// `abstract_interp::engine::build_function_table` — the exact per-function
/// work `InferenceEngine::add_functions` would otherwise redo on the
/// compiling thread for ~5000 Base+prelude functions on every compile. The
/// CLI then warms the Base cache on the main thread via `warm_base_cache`
/// while the background thread does this work, overlapping the two largest
/// deserializes with it. Every consumer falls back to the regular path when
/// the prefetch is absent, failed, or mismatched.
#[cfg(not(target_arch = "wasm32"))]
mod warm_prefetch {
    use std::collections::{HashMap, HashSet};
    use std::sync::Mutex;
    use std::thread::JoinHandle;

    /// Prefetched Base+prelude inference state (Issue #10114): the
    /// `(function_table, ambiguous_functions)` pair
    /// `InferenceEngine::add_functions` would produce for the Base+prelude
    /// function slice, plus `len` (the slice length) so a consumer can check
    /// it still matches `base_function_count` before trusting the snapshot.
    pub(super) struct PrefetchedInferenceState {
        pub(super) function_table: HashMap<String, crate::ir::core::Function>,
        pub(super) ambiguous_functions: HashSet<String>,
        pub(super) len: usize,
    }

    pub(super) static INFERENCE_FNS_PREFETCH: Mutex<
        Option<JoinHandle<Option<PrefetchedInferenceState>>>,
    > = Mutex::new(None);

    pub(super) fn join<T>(slot: &Mutex<Option<JoinHandle<Option<T>>>>) -> Option<T> {
        let handle = slot.lock().ok()?.take()?;
        handle.join().ok()?
    }
}

/// Spawn a background thread that pre-loads warm-start artifacts
/// (Issue #6348 / #10114): the prelude `Program` Lazy and the Base+prelude
/// function-table snapshot the shared inference engine seeds from each
/// compile.
///
/// Safe to call multiple times (later calls are no-ops while a prefetch is
/// pending) and safe to never consume (the thread finishes and the artifacts
/// are dropped). No thread-local compile state is touched off-thread.
#[cfg(not(target_arch = "wasm32"))]
pub fn begin_warm_start_prefetch() {
    if is_cache_disabled() {
        return;
    }

    if let Ok(mut slot) = warm_prefetch::INFERENCE_FNS_PREFETCH.lock() {
        if slot.is_none() {
            *slot = Some(std::thread::spawn(|| {
                // Warming the Lazy here is the point: the main thread's
                // `parse_and_lower` blocks on the same `once_cell` until the
                // prelude Program is ready.
                let prelude = crate::get_prelude_program()?;
                let len = prelude.functions.len();
                // The shared inference engine consumes owned `Function` values
                // (Issue #6348), so this background thread still pays the real
                // deep clone here — deliberately overlapped with the main
                // thread's Base-cache load — even though `Program.functions`
                // is now `Arc<Function>` (Issue #9140) and a plain `.clone()`
                // would be a cheap refcount bump rather than the clone this
                // prefetch exists to hide. Issue #10114: also do the
                // function-table insertion work here (not just the clone),
                // since that dominates `compile.build_inference_engine`.
                let (function_table, ambiguous_functions) =
                    crate::compile::abstract_interp::engine::build_function_table(
                        prelude.functions.iter().map(|f| (**f).clone()),
                    );
                Some(warm_prefetch::PrefetchedInferenceState {
                    function_table,
                    ambiguous_functions,
                    len,
                })
            }));
        }
    }
}

/// No-op on wasm (no threads).
#[cfg(target_arch = "wasm32")]
pub fn begin_warm_start_prefetch() {}

/// Eagerly initialize the thread-local Base cache on the calling thread
/// (Issue #6348).
///
/// The CLI calls this before `parse_and_lower` so the ~9 ms Base-cache
/// read + deserialize overlaps with the background prelude load started by
/// `begin_warm_start_prefetch`. Errors are deliberately swallowed: the
/// regular compile path will retry and report them.
pub fn warm_base_cache() {
    if is_cache_disabled() {
        return;
    }
    let _ = get_or_init_base_cache();
}

/// Take the prefetched Base+prelude `(function_table, ambiguous_functions)`
/// snapshot if it matches the expected Base segment length (at most once per
/// process; Issue #10114, replacing the raw-`Vec<Function>` prefetch this
/// building on top of `build_function_table` superseded).
#[cfg(not(target_arch = "wasm32"))]
pub(super) fn take_prefetched_base_function_table(
    expected_len: usize,
) -> Option<(HashMap<String, crate::ir::core::Function>, HashSet<String>)> {
    let prefetched = warm_prefetch::join(&warm_prefetch::INFERENCE_FNS_PREFETCH)?;
    (prefetched.len == expected_len)
        .then_some((prefetched.function_table, prefetched.ambiguous_functions))
}

#[cfg(target_arch = "wasm32")]
pub(super) fn take_prefetched_base_function_table(
    _expected_len: usize,
) -> Option<(HashMap<String, crate::ir::core::Function>, HashSet<String>)> {
    None
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
    let base_function_replaced =
        program.base_function_count > 0 && program.base_function_count != prelude_function_count;
    base_function_replaced
        || program_user_functions_shadow_base_kwparams(program)
        || program_extends_promotion_rules_over_base_types(program)
        || program_extends_iterator_traits(program)
        || program_extends_dict_view_functions(program)
        || program_main_contains_block_function_defs(program)
}

/// Return true when a user method shadows a Base keyword-parameter callable name.
///
/// Cached Base bytecode can contain calls through captured keyword parameters
/// (for example `retry(...; check=...)`). If user code later defines a function
/// with the same name, compile-time resolution can see the user method table
/// instead of the captured callable. Full compilation keeps the callable slot
/// path intact (Issue #8469).
fn program_user_functions_shadow_base_kwparams(program: &Program) -> bool {
    if program.base_function_count == 0 {
        return false;
    }

    let base_kwparam_names: HashSet<&str> = program
        .functions
        .iter()
        .take(program.base_function_count)
        .flat_map(|func| func.kwparams.iter().map(|kw| kw.name.as_str()))
        .collect();
    if base_kwparam_names.is_empty() {
        return false;
    }

    program
        .functions
        .iter()
        .skip(program.base_function_count)
        .any(|func| base_kwparam_names.contains(func.name.as_str()))
        || block_function_names_shadow_kwparams(&program.main, &base_kwparam_names)
}

fn block_function_names_shadow_kwparams(block: &Block, base_kwparam_names: &HashSet<&str>) -> bool {
    block
        .stmts
        .iter()
        .any(|stmt| stmt_function_names_shadow_kwparams(stmt, base_kwparam_names))
}

fn stmt_function_names_shadow_kwparams(stmt: &Stmt, base_kwparam_names: &HashSet<&str>) -> bool {
    match stmt {
        Stmt::FunctionDef { func, .. } => {
            base_kwparam_names.contains(func.name.as_str())
                || block_function_names_shadow_kwparams(&func.body, base_kwparam_names)
        }
        Stmt::Assign { value, .. } | Stmt::AddAssign { value, .. } => {
            expr_function_names_shadow_kwparams(value, base_kwparam_names)
        }
        Stmt::Block(block)
        | Stmt::Timed { body: block, .. }
        | Stmt::TestSet { body: block, .. } => {
            block_function_names_shadow_kwparams(block, base_kwparam_names)
        }
        Stmt::For {
            start,
            end,
            step,
            body,
            ..
        } => {
            expr_function_names_shadow_kwparams(start, base_kwparam_names)
                || step.as_ref().is_some_and(|expr| {
                    expr_function_names_shadow_kwparams(expr, base_kwparam_names)
                })
                || expr_function_names_shadow_kwparams(end, base_kwparam_names)
                || block_function_names_shadow_kwparams(body, base_kwparam_names)
        }
        Stmt::ForEach { iterable, body, .. } => {
            expr_function_names_shadow_kwparams(iterable, base_kwparam_names)
                || block_function_names_shadow_kwparams(body, base_kwparam_names)
        }
        Stmt::ForEachTuple { iterable, body, .. } => {
            expr_function_names_shadow_kwparams(iterable, base_kwparam_names)
                || block_function_names_shadow_kwparams(body, base_kwparam_names)
        }
        Stmt::While {
            condition, body, ..
        } => {
            expr_function_names_shadow_kwparams(condition, base_kwparam_names)
                || block_function_names_shadow_kwparams(body, base_kwparam_names)
        }
        Stmt::If {
            condition,
            then_branch,
            else_branch,
            ..
        } => {
            expr_function_names_shadow_kwparams(condition, base_kwparam_names)
                || block_function_names_shadow_kwparams(then_branch, base_kwparam_names)
                || else_branch.as_ref().is_some_and(|block| {
                    block_function_names_shadow_kwparams(block, base_kwparam_names)
                })
        }
        Stmt::Try {
            try_block,
            catch_block,
            else_block,
            finally_block,
            ..
        } => {
            block_function_names_shadow_kwparams(try_block, base_kwparam_names)
                || catch_block.as_ref().is_some_and(|block| {
                    block_function_names_shadow_kwparams(block, base_kwparam_names)
                })
                || else_block.as_ref().is_some_and(|block| {
                    block_function_names_shadow_kwparams(block, base_kwparam_names)
                })
                || finally_block.as_ref().is_some_and(|block| {
                    block_function_names_shadow_kwparams(block, base_kwparam_names)
                })
        }
        Stmt::Return { value, .. } => value
            .as_ref()
            .is_some_and(|expr| expr_function_names_shadow_kwparams(expr, base_kwparam_names)),
        Stmt::Expr { expr, .. }
        | Stmt::Test {
            condition: expr, ..
        } => expr_function_names_shadow_kwparams(expr, base_kwparam_names),
        Stmt::TestThrows { expr, .. } => {
            expr_function_names_shadow_kwparams(expr, base_kwparam_names)
        }
        Stmt::IndexAssign { indices, value, .. } => {
            indices
                .iter()
                .any(|expr| expr_function_names_shadow_kwparams(expr, base_kwparam_names))
                || expr_function_names_shadow_kwparams(value, base_kwparam_names)
        }
        Stmt::FieldAssign { value, .. } | Stmt::DictAssign { value, .. } => {
            expr_function_names_shadow_kwparams(value, base_kwparam_names)
        }
        Stmt::DestructuringAssign { value, .. } => {
            expr_function_names_shadow_kwparams(value, base_kwparam_names)
        }
        _ => false,
    }
}

fn expr_function_names_shadow_kwparams(expr: &Expr, base_kwparam_names: &HashSet<&str>) -> bool {
    match expr {
        Expr::BinaryOp { left, right, .. } => {
            expr_function_names_shadow_kwparams(left, base_kwparam_names)
                || expr_function_names_shadow_kwparams(right, base_kwparam_names)
        }
        Expr::UnaryOp { operand, .. } | Expr::Convert { operand, .. } => {
            expr_function_names_shadow_kwparams(operand, base_kwparam_names)
        }
        Expr::Call { args, kwargs, .. } | Expr::ModuleCall { args, kwargs, .. } => {
            args.iter()
                .any(|expr| expr_function_names_shadow_kwparams(expr, base_kwparam_names))
                || kwargs
                    .iter()
                    .any(|(_, expr)| expr_function_names_shadow_kwparams(expr, base_kwparam_names))
        }
        Expr::Builtin { args, .. }
        | Expr::ArrayLiteral { elements: args, .. }
        | Expr::TupleLiteral { elements: args, .. }
        | Expr::StringConcat { parts: args, .. }
        | Expr::New { args, .. } => args
            .iter()
            .any(|expr| expr_function_names_shadow_kwparams(expr, base_kwparam_names)),
        Expr::Index { array, indices, .. } => {
            expr_function_names_shadow_kwparams(array, base_kwparam_names)
                || indices
                    .iter()
                    .any(|expr| expr_function_names_shadow_kwparams(expr, base_kwparam_names))
        }
        Expr::Range {
            start, step, stop, ..
        } => {
            expr_function_names_shadow_kwparams(start, base_kwparam_names)
                || step.as_ref().is_some_and(|expr| {
                    expr_function_names_shadow_kwparams(expr, base_kwparam_names)
                })
                || expr_function_names_shadow_kwparams(stop, base_kwparam_names)
        }
        Expr::Comprehension {
            body, iter, filter, ..
        }
        | Expr::Generator {
            body, iter, filter, ..
        } => {
            expr_function_names_shadow_kwparams(body, base_kwparam_names)
                || expr_function_names_shadow_kwparams(iter, base_kwparam_names)
                || filter.as_ref().is_some_and(|expr| {
                    expr_function_names_shadow_kwparams(expr, base_kwparam_names)
                })
        }
        Expr::MultiComprehension {
            body,
            iterations,
            filter,
            ..
        } => {
            expr_function_names_shadow_kwparams(body, base_kwparam_names)
                || iterations
                    .iter()
                    .any(|(_, expr)| expr_function_names_shadow_kwparams(expr, base_kwparam_names))
                || filter.as_ref().is_some_and(|expr| {
                    expr_function_names_shadow_kwparams(expr, base_kwparam_names)
                })
        }
        Expr::FieldAccess { object, .. } => {
            expr_function_names_shadow_kwparams(object, base_kwparam_names)
        }
        Expr::NamedTupleLiteral { fields, .. } => fields
            .iter()
            .any(|(_, expr)| expr_function_names_shadow_kwparams(expr, base_kwparam_names)),
        Expr::Pair { key, value, .. } => {
            expr_function_names_shadow_kwparams(key, base_kwparam_names)
                || expr_function_names_shadow_kwparams(value, base_kwparam_names)
        }
        Expr::DictLiteral { pairs, .. } => pairs.iter().any(|(key, value)| {
            expr_function_names_shadow_kwparams(key, base_kwparam_names)
                || expr_function_names_shadow_kwparams(value, base_kwparam_names)
        }),
        Expr::LetBlock { bindings, body, .. } => {
            bindings
                .iter()
                .any(|(_, expr)| expr_function_names_shadow_kwparams(expr, base_kwparam_names))
                || block_function_names_shadow_kwparams(body, base_kwparam_names)
        }
        Expr::Ternary {
            condition,
            then_expr,
            else_expr,
            ..
        } => {
            expr_function_names_shadow_kwparams(condition, base_kwparam_names)
                || expr_function_names_shadow_kwparams(then_expr, base_kwparam_names)
                || expr_function_names_shadow_kwparams(else_expr, base_kwparam_names)
        }
        Expr::DynamicTypeConstruct {
            base_expr,
            type_args,
            ..
        } => {
            base_expr
                .as_ref()
                .is_some_and(|expr| expr_function_names_shadow_kwparams(expr, base_kwparam_names))
                || type_args
                    .iter()
                    .any(|expr| expr_function_names_shadow_kwparams(expr, base_kwparam_names))
        }
        Expr::QuoteLiteral { constructor, .. } => {
            expr_function_names_shadow_kwparams(constructor, base_kwparam_names)
        }
        Expr::AssignExpr { value, .. } => {
            expr_function_names_shadow_kwparams(value, base_kwparam_names)
        }
        Expr::ReturnExpr { value, .. } => value
            .as_ref()
            .is_some_and(|expr| expr_function_names_shadow_kwparams(expr, base_kwparam_names)),
        Expr::Literal(..)
        | Expr::Var(..)
        | Expr::TypedEmptyArray { .. }
        | Expr::SliceAll { .. }
        | Expr::FunctionRef { .. }
        | Expr::BreakExpr { .. }
        | Expr::ContinueExpr { .. } => false,
    }
}

/// Base generic-function names whose "disable the whole Base cache" fallback
/// has been retired (Issue #8555 for `promote_rule`/iterator traits, Issue
/// #8602 for `keys`/`values`/`pairs`; slices of #8442).
///
/// When a user program extends one of these hooks with methods anchored to a
/// program-local type, the Base cache is loaded normally and the frozen
/// dynamic-dispatch candidate lists inside the cached Base bytecode are
/// refreshed post-merge with the user's method indices
/// (`pipeline_ctx::refresh_cached_base_dispatch_candidates`) — the targeted
/// analogue of what a full recompile would bake in.
pub(super) const BASE_DISPATCH_REFRESH_HOOKS: &[&str] = &[
    "promote_rule",
    "IteratorEltype",
    "IteratorSize",
    "eltype",
    "keys",
    "values",
    "pairs",
];

/// Return true when user code extends promotion hooks in a way the cached
/// Base bytecode could depend on statically.
///
/// Upstream Julia's `promote_type` calls `promote_rule` through ordinary multiple
/// dispatch (`julia/base/promotion.jl`). Cached sjulia Base bytecode carries
/// compile-time-frozen dispatch state for `promote_rule` (Issue #4048), split in
/// two classes (Issue #8555):
///
/// - **Dynamic dispatch sites** (`CallTypedDispatch`-family candidate lists):
///   refreshed post-merge with the user's method indices, so a user
///   `promote_rule` method anchored to a *program-local* type (a type the Base
///   prelude has never seen — user struct or bundled-package type) is fully
///   covered without recompiling Base.
/// - **Static folds** over Base-known type pairs (inference constant-folded
///   `promote_type(Int64, Float64)`-style results inside Base bytecode): a
///   *pirating* method — one whose signature could match a pair of Base-known
///   types — can change those, and no post-load patch can reach them. Only that
///   class still forces the full-compile bypass.
///
/// A method is safely anchored when at least one parameter slot can only ever
/// match a program-local type; then every dispatch match involves a type that
/// did not exist when Base was compiled, so no cached static resolution can be
/// affected. Anything ambiguous errs toward the bypass (over-invalidation).
fn program_extends_promotion_rules_over_base_types(program: &Program) -> bool {
    program
        .functions
        .iter()
        .skip(program.base_function_count)
        .filter(|func| func.name == "promote_rule")
        .any(|func| !function_signature_requires_program_local_type(func))
}

/// True when at least one declared parameter slot of `func` can only match a
/// program-local (non-Base-prelude) type — see
/// [`program_extends_promotion_rules_over_base_types`].
fn function_signature_requires_program_local_type(func: &crate::ir::core::Function) -> bool {
    func.params
        .iter()
        .any(|param| julia_type_requires_program_local_type(&param.effective_type()))
}

/// True when every value matching `ty` necessarily involves a program-local
/// type. `Type{X}` looks through to `X`; a `Union` requires *all* members to
/// be program-local (a single Base-known member lets the slot match without
/// any program-local type). Type variables and anything unrecognized answer
/// `false` (not anchored — the conservative direction, toward the bypass).
fn julia_type_requires_program_local_type(ty: &crate::types::JuliaType) -> bool {
    use crate::types::JuliaType;
    match ty {
        JuliaType::TypeOf(inner) => julia_type_requires_program_local_type(inner),
        JuliaType::Struct(name) => type_name_is_program_local(name),
        JuliaType::Union(members) => {
            !members.is_empty() && members.iter().all(julia_type_requires_program_local_type)
        }
        _ => false,
    }
}

/// True when `name` denotes a type the cached Base prelude has never seen:
/// not a compiler-builtin type name and not declared anywhere in the prelude
/// IR (top level or inside prelude modules). Module-qualified names are
/// conservatively treated as Base-known.
fn type_name_is_program_local(name: &str) -> bool {
    let base = name.split('{').next().unwrap_or(name).trim();
    if base.is_empty() || base.contains('.') {
        return false;
    }
    if crate::types::JuliaType::from_name(base).is_some() {
        return false;
    }
    match prelude_declared_type_names() {
        Some(names) => !names.contains(base),
        // Prelude unavailable: cannot prove the type is program-local, so
        // treat it as Base-known (drives the caller toward the bypass).
        None => false,
    }
}

/// Type names declared by the Base prelude IR (structs, abstract types,
/// primitive types, type aliases, enums; recursively through prelude
/// modules). Computed once per process.
fn prelude_declared_type_names() -> Option<&'static HashSet<String>> {
    static PRELUDE_TYPE_NAMES: std::sync::OnceLock<Option<HashSet<String>>> =
        std::sync::OnceLock::new();
    PRELUDE_TYPE_NAMES
        .get_or_init(|| {
            let prelude = crate::get_prelude_program()?;
            let mut names = HashSet::new();
            collect_declared_type_names(
                &prelude.structs,
                &prelude.abstract_types,
                &prelude.primitive_types,
                &prelude.type_aliases,
                &mut names,
            );
            for enum_def in &prelude.enums {
                names.insert(enum_def.name.clone());
            }
            let mut modules: Vec<&Module> = prelude.modules.iter().collect();
            while let Some(module) = modules.pop() {
                collect_declared_type_names(
                    &module.structs,
                    &module.abstract_types,
                    &module.primitive_types,
                    &module.type_aliases,
                    &mut names,
                );
                modules.extend(module.submodules.iter());
            }
            Some(names)
        })
        .as_ref()
}

fn collect_declared_type_names(
    structs: &[crate::ir::core::StructDef],
    abstract_types: &[crate::ir::core::AbstractTypeDef],
    primitive_types: &[crate::ir::core::PrimitiveTypeDef],
    type_aliases: &[TypeAliasDef],
    names: &mut HashSet<String>,
) {
    names.extend(structs.iter().map(|def| def.name.clone()));
    names.extend(abstract_types.iter().map(|def| def.name.clone()));
    names.extend(primitive_types.iter().map(|def| def.name.clone()));
    names.extend(type_aliases.iter().map(|def| def.name.clone()));
}

/// Return true when user code extends iterator trait hooks in a way the
/// cached Base bytecode could depend on statically.
///
/// Cached Base bytecode carries frozen dispatch state for
/// `IteratorEltype`/`IteratorSize` used by `collect`/`_collect` dispatch
/// (Issue #4088). Like `promote_rule` (Issue #8555), methods anchored to a
/// program-local type are covered by the post-merge candidate refresh
/// (`pipeline_ctx::refresh_cached_base_dispatch_candidates`); only methods
/// that could capture Base-known types (piracy / unanchored signatures) still
/// force the full-compile bypass.
fn program_extends_iterator_traits(program: &Program) -> bool {
    program
        .functions
        .iter()
        .skip(program.base_function_count)
        .filter(|func| matches!(func.name.as_str(), "IteratorEltype" | "IteratorSize"))
        .any(|func| !function_signature_requires_program_local_type(func))
}

/// Return true when user code extends Dict view hooks in a way the cached
/// Base bytecode could depend on statically.
///
/// `keys` / `values` / `pairs` are dispatch-first Base functions with retained
/// Rust-backed Dict view fallbacks
/// (`CallTypedDispatchOrBuiltin(DictKeys/DictValues/DictPairs, ..)`). Those
/// sites carry the function name, so — like `promote_rule` (Issue #8555) —
/// methods anchored to a program-local type are covered by the post-merge
/// candidate refresh (`pipeline_ctx::refresh_cached_base_dispatch_candidates`)
/// and no longer disable the Base cache (Issue #8602 retires the #4671
/// bypass). The name-less `CallBuiltin(DictKeys/DictValues/DictPairs, 1)`
/// sites are emitted only for receivers statically known to be Base-native
/// pairs-view types (arrays/tuples/NamedTuples — `is_pairs_view_arg_type`),
/// which an anchored user method can never match, so they need no refresh.
///
/// Only methods that could capture Base-known types keep the full-compile
/// bypass — e.g. the `keys(::Dict{String,Float64})` piracy #4671 was
/// originally filed about, which cached static resolutions over Base-known
/// Dict instantiations could otherwise ignore.
fn program_extends_dict_view_functions(program: &Program) -> bool {
    program
        .functions
        .iter()
        .skip(program.base_function_count)
        .filter(|func| matches!(func.name.as_str(), "keys" | "values" | "pairs"))
        .any(|func| !function_signature_requires_program_local_type(func))
}

/// Return true when user main contains a *named* block-local function
/// definition.
///
/// Cached Base bytecode carries method-table visibility from the precompiled
/// Base segment. A later block-local `Stmt::FunctionDef` in user main can be
/// referenced through generic Base helpers such as `@testset`; compiling the
/// whole program keeps those local methods visible in the same method table
/// used to compile the call site (Issue #8469).
///
/// Compiler-generated *anonymous* callables that the lowering lifts into main
/// (`__lambda_*` arrows/`do`-blocks, `__gen_body_*` generators) are EXCLUDED:
/// their gensym names are unique and can never be referenced by name through a
/// Base helper, never replace a Base method, and never shift
/// `base_function_count`, so bypassing the cache for them was pure, unnecessary
/// cost — a top-level `map(x -> …)` / `surface(x, y, (x, y) -> …)` /
/// `sum(x for x in …)` re-inferred all ~4,900 Base functions (Issue #9250).
///
/// The one exception the anonymous exclusion must NOT swallow: calling a Base
/// *closure factory* (`retry`, curried `isequal` — see
/// [`base_fn_returns_anonymous_closure`]). Those still force the whole-program
/// compile even when main's only function value is an anonymous lambda, so the
/// scan also returns true for a call to any [`base_closure_factory_names`] entry
/// (Issue #9250 must not re-expose the #8469 captured-kwparam hazard).
fn program_main_contains_block_function_defs(program: &Program) -> bool {
    let user_function_names: HashSet<&str> = program
        .functions
        .iter()
        .skip(program.base_function_count)
        // A lifted anonymous callable extracted here (referenced from main as
        // `Expr::FunctionRef("__lambda_0")`) is a value, not a named method
        // reachable through a Base helper, so it must not seed the reference
        // check below (Issue #9250).
        .filter(|func| !crate::compile::ir_inline::is_markerless_lowered_function(func))
        .map(|func| func.name.as_str())
        .collect();
    let factory_names = base_closure_factory_names(program);
    let ctx = MainDefScan {
        user_function_names: &user_function_names,
        factory_names: &factory_names,
    };
    block_contains_function_def(&program.main, &ctx)
}

/// Threaded state for the main-block function-def / factory-call scan.
struct MainDefScan<'a> {
    /// Named user functions (compiler-generated anonymous names excluded, so a
    /// bare `map(x -> …)` / `surface(x, y, (x, y) -> …)` reference does not force
    /// a whole-program compile — Issue #9250).
    user_function_names: &'a HashSet<&'a str>,
    /// Base closure-factory names (`retry`, curried `isequal`): calling one is a
    /// #8469 hazard, so it keeps the whole-program compile even for an
    /// anonymous-only main (Issue #9250).
    factory_names: &'a HashSet<&'a str>,
}

/// Base functions whose *result* is a compiler-generated anonymous closure —
/// closure factories such as `retry(f; delays=…, check=…)` and curried
/// `isequal(x)`. Their `CreateClosure { capture_names: [...] }` captures the
/// factory's locals/kwparams; the cached Base bytecode then calls through those
/// captures, which the base-cache path cannot rewire (Issue #8469). A HOF that
/// merely *calls* its function argument (`map`, `filter`, `surface`) is NOT a
/// factory — this checks the RETURN value only, so those stay on the fast
/// cached-Base path (Issue #9250).
fn base_closure_factory_names(program: &Program) -> HashSet<&str> {
    let n = program.base_function_count.min(program.functions.len());
    program.functions[..n]
        .iter()
        .filter(|func| base_fn_returns_anonymous_closure(func))
        .map(|func| func.name.as_str())
        .collect()
}

/// True when `expr` evaluates to a compiler-generated anonymous closure. The
/// lowering wraps `return (args...) -> …` as `Return(LetBlock { body: [
/// FunctionDef(__lambda_nested_N), Var("__lambda_nested_N") ] })`, so this
/// unwraps a `LetBlock`/trailing statement down to the `Var`/`FunctionRef` leaf.
fn expr_is_anonymous_closure(expr: &Expr) -> bool {
    match expr {
        Expr::Var(name, _) | Expr::FunctionRef { name, .. } => {
            is_compiler_generated_anonymous_def(name)
        }
        Expr::LetBlock { body, .. } => block_tail_is_anonymous_closure(body),
        _ => false,
    }
}

fn block_tail_is_anonymous_closure(block: &Block) -> bool {
    match block.stmts.last() {
        Some(Stmt::Expr { expr, .. }) => expr_is_anonymous_closure(expr),
        Some(Stmt::Return { value: Some(e), .. }) => expr_is_anonymous_closure(e),
        _ => false,
    }
}

/// A Base function whose *result* is a compiler-generated anonymous closure —
/// a closure factory such as `retry(f; delays=…, check=…)` and curried
/// `isequal(x)`. Its returned closure captures the factory's kwparams/locals
/// (`CreateClosure { capture_names: [...] }`); the cached Base bytecode then
/// calls through those captures, which the base-cache path cannot rewire
/// (Issue #8469). A HOF that merely *calls* its function argument (`map`,
/// `filter`, `surface`) is NOT a factory — only the RETURN value is inspected —
/// so those stay on the fast cached-Base path (Issue #9250).
fn base_fn_returns_anonymous_closure(func: &Function) -> bool {
    func.body.stmts.iter().any(
        |stmt| matches!(stmt, Stmt::Return { value: Some(e), .. } if expr_is_anonymous_closure(e)),
    ) || block_tail_is_anonymous_closure(&func.body)
}

/// `true` for the compiler-generated *anonymous* function names the lowering
/// lifts into `main` as `Stmt::FunctionDef` values: arrows and `do`-blocks
/// (`__lambda_*`, `__do_block_*`) and generator bodies (`__gen_body_*`), in both
/// the bare and qualified (`parent#…#__lambda_N`) forms. These are passed as
/// first-class values / driven by the iterator machinery, never resolved by
/// name from cached Base bytecode, so they must not trip the Base-cache bypass
/// (Issue #9250). This spelling check is restricted to inspecting returned
/// closures inside Base bodies, where the lowering owns every such name. User
/// definition classification uses `Span.definition_order` provenance instead
/// (Issue #9784).
fn is_compiler_generated_anonymous_def(name: &str) -> bool {
    let leaf = name.rsplit('#').next().unwrap_or(name);
    leaf.starts_with("__lambda_")
        || leaf.starts_with("__do_block_")
        || leaf.starts_with("__gen_body_")
        || leaf.starts_with("__gen_pred_")
}

fn block_contains_function_def(block: &Block, ctx: &MainDefScan<'_>) -> bool {
    block
        .stmts
        .iter()
        .any(|stmt| stmt_contains_function_def(stmt, ctx))
}

fn stmt_contains_function_def(stmt: &Stmt, ctx: &MainDefScan<'_>) -> bool {
    match stmt {
        // A genuinely named block-local method (the #8469 `@testset` case) still
        // forces a whole-program compile; a compiler-generated anonymous lambda /
        // `do`-block / generator body must NOT (Issue #9250).
        Stmt::FunctionDef { func, .. } => {
            !crate::compile::ir_inline::is_markerless_lowered_function(func)
        }
        Stmt::Block(block)
        | Stmt::Timed { body: block, .. }
        | Stmt::TestSet { body: block, .. }
        | Stmt::While { body: block, .. }
        | Stmt::For { body: block, .. }
        | Stmt::ForEach { body: block, .. }
        | Stmt::ForEachTuple { body: block, .. } => block_contains_function_def(block, ctx),
        Stmt::If {
            condition,
            then_branch,
            else_branch,
            ..
        } => {
            expr_contains_function_def(condition, ctx)
                || block_contains_function_def(then_branch, ctx)
                || else_branch
                    .as_ref()
                    .is_some_and(|block| block_contains_function_def(block, ctx))
        }
        Stmt::Try {
            try_block,
            catch_block,
            else_block,
            finally_block,
            ..
        } => {
            block_contains_function_def(try_block, ctx)
                || catch_block
                    .as_ref()
                    .is_some_and(|block| block_contains_function_def(block, ctx))
                || else_block
                    .as_ref()
                    .is_some_and(|block| block_contains_function_def(block, ctx))
                || finally_block
                    .as_ref()
                    .is_some_and(|block| block_contains_function_def(block, ctx))
        }
        Stmt::Assign { value, .. } | Stmt::AddAssign { value, .. } => {
            expr_contains_function_def(value, ctx)
        }
        Stmt::Return { value, .. } => value
            .as_ref()
            .is_some_and(|expr| expr_contains_function_def(expr, ctx)),
        Stmt::Expr { expr, .. }
        | Stmt::Test {
            condition: expr, ..
        } => expr_contains_function_def(expr, ctx),
        Stmt::TestThrows { expr, .. } => expr_contains_function_def(expr, ctx),
        Stmt::IndexAssign { indices, value, .. } => {
            indices
                .iter()
                .any(|expr| expr_contains_function_def(expr, ctx))
                || expr_contains_function_def(value, ctx)
        }
        Stmt::FieldAssign { value, .. } => expr_contains_function_def(value, ctx),
        Stmt::DestructuringAssign { value, .. } => expr_contains_function_def(value, ctx),
        Stmt::DictAssign { key, value, .. } => {
            expr_contains_function_def(key, ctx) || expr_contains_function_def(value, ctx)
        }
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
        | Stmt::RuntimeNominalDef { .. }
        | Stmt::Global { .. } => false,
    }
}

fn expr_contains_function_def(expr: &Expr, ctx: &MainDefScan<'_>) -> bool {
    match expr {
        Expr::BinaryOp { left, right, .. } => {
            expr_contains_function_def(left, ctx) || expr_contains_function_def(right, ctx)
        }
        Expr::UnaryOp { operand, .. }
        | Expr::Convert { operand, .. }
        | Expr::FieldAccess {
            object: operand, ..
        }
        | Expr::QuoteLiteral {
            constructor: operand,
            ..
        }
        | Expr::AssignExpr { value: operand, .. } => expr_contains_function_def(operand, ctx),
        Expr::Call {
            function,
            args,
            kwargs,
            ..
        } => {
            // Calling a Base closure-factory (`retry`, curried `isequal`) keeps
            // the whole-program compile even when main's only function defs are
            // anonymous lambdas: the factory's cached bytecode captures its
            // kwparams/locals into the returned closure, which the base-cache
            // path cannot rewire (Issue #8469 / #9250 narrowing).
            ctx.factory_names.contains(function.as_str())
                || args
                    .iter()
                    .any(|expr| expr_contains_function_def(expr, ctx))
                || kwargs
                    .iter()
                    .any(|(_, value)| expr_contains_function_def(value, ctx))
        }
        Expr::ModuleCall { args, kwargs, .. } => {
            args.iter()
                .any(|expr| expr_contains_function_def(expr, ctx))
                || kwargs
                    .iter()
                    .any(|(_, value)| expr_contains_function_def(value, ctx))
        }
        Expr::Builtin { args, .. } | Expr::ArrayLiteral { elements: args, .. } => args
            .iter()
            .any(|expr| expr_contains_function_def(expr, ctx)),
        Expr::Index { array, indices, .. } => {
            expr_contains_function_def(array, ctx)
                || indices
                    .iter()
                    .any(|expr| expr_contains_function_def(expr, ctx))
        }
        Expr::Range {
            start, step, stop, ..
        } => {
            expr_contains_function_def(start, ctx)
                || step
                    .as_ref()
                    .is_some_and(|step| expr_contains_function_def(step, ctx))
                || expr_contains_function_def(stop, ctx)
        }
        Expr::Comprehension {
            body, iter, filter, ..
        }
        | Expr::Generator {
            body, iter, filter, ..
        } => {
            expr_contains_function_def(body, ctx)
                || expr_contains_function_def(iter, ctx)
                || filter
                    .as_ref()
                    .is_some_and(|filter| expr_contains_function_def(filter, ctx))
        }
        Expr::MultiComprehension {
            body,
            iterations,
            filter,
            ..
        } => {
            expr_contains_function_def(body, ctx)
                || iterations
                    .iter()
                    .any(|(_, iter)| expr_contains_function_def(iter, ctx))
                || filter
                    .as_ref()
                    .is_some_and(|filter| expr_contains_function_def(filter, ctx))
        }
        Expr::TupleLiteral { elements, .. }
        | Expr::StringConcat {
            parts: elements, ..
        } => elements
            .iter()
            .any(|expr| expr_contains_function_def(expr, ctx)),
        Expr::NamedTupleLiteral { fields, .. } => fields
            .iter()
            .any(|(_, value)| expr_contains_function_def(value, ctx)),
        Expr::Pair { key, value, .. } => {
            expr_contains_function_def(key, ctx) || expr_contains_function_def(value, ctx)
        }
        Expr::DictLiteral { pairs, .. } => pairs.iter().any(|(key, value)| {
            expr_contains_function_def(key, ctx) || expr_contains_function_def(value, ctx)
        }),
        Expr::LetBlock { bindings, body, .. } => {
            bindings
                .iter()
                .any(|(_, value)| expr_contains_function_def(value, ctx))
                || block_contains_function_def(body, ctx)
        }
        Expr::Ternary {
            condition,
            then_expr,
            else_expr,
            ..
        } => {
            expr_contains_function_def(condition, ctx)
                || expr_contains_function_def(then_expr, ctx)
                || expr_contains_function_def(else_expr, ctx)
        }
        Expr::New { args, .. } => args
            .iter()
            .any(|expr| expr_contains_function_def(expr, ctx)),
        Expr::DynamicTypeConstruct {
            base_expr,
            type_args,
            ..
        } => {
            base_expr
                .as_ref()
                .is_some_and(|base| expr_contains_function_def(base, ctx))
                || type_args
                    .iter()
                    .any(|expr| expr_contains_function_def(expr, ctx))
        }
        Expr::ReturnExpr { value, .. } => value
            .as_ref()
            .is_some_and(|value| expr_contains_function_def(value, ctx)),
        Expr::FunctionRef { name, .. } => ctx.user_function_names.contains(name.as_str()),
        Expr::Literal(..)
        | Expr::Var(..)
        | Expr::TypedEmptyArray { .. }
        | Expr::SliceAll { .. }
        | Expr::BreakExpr { .. }
        | Expr::ContinueExpr { .. } => false,
    }
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
    /// Inference return cache snapshot from Base compilation.
    ///
    /// Stored in `CachedBase` for same-process source-compiled hits only. The
    /// persistent/embedded serialized payload intentionally omits these snapshots
    /// and the load path drops any legacy non-empty payloads (Issue #6348).
    inference_results: Vec<(InferenceCacheKey, CachedReturn)>,
}

struct PersistentCacheLock {
    path: PathBuf,
}

impl Drop for PersistentCacheLock {
    fn drop(&mut self) {
        let _ = fs::remove_file(&self.path);
    }
}

thread_local! {
    /// Thread-local cache for Base compilation (bytecode + method tables).
    /// Wrapped in `Rc` to avoid deep-cloning on every retrieval (Issue #3357).
    static BASE_CACHE: RefCell<Option<Rc<CachedBase>>> = const { RefCell::new(None) };

    /// Thread-local cache for full programs (keyed by program hash)
    /// Useful for benchmarks and repeated compilations of identical code
    static PROGRAM_CACHE: RefCell<HashMap<u64, CompiledProgram>> = RefCell::new(HashMap::new());

    /// Program hashes that have been compiled at least once in this thread.
    ///
    /// `PROGRAM_CACHE` stores a deep clone of the final `CompiledProgram`
    /// (~6 ms for a Base-merged program). One-shot CLI runs never benefit from
    /// that clone, so the cache only stores a program on its SECOND compile
    /// (first compile just records the hash here). Repeated compilations —
    /// the cache's actual use case — still get full hits from the third
    /// compile onward (Issue #6348).
    static PROGRAM_CACHE_SEEN: RefCell<std::collections::HashSet<u64>> =
        RefCell::new(std::collections::HashSet::new());

    /// Build-time-embedded seeded entries (Issue #10120), as `(hash, raw
    /// postcard-encoded CompiledProgram bytes)` -- deliberately NOT decoded
    /// eagerly. Each `CompiledProgram` is a full Base-merged compile (a few
    /// MB); decoding every embedded seed up front would cost several times a
    /// single Base-cache decode, more than the decode Issue #10118 just
    /// optimized. `seeded_program_cache_lookup` decodes ON DEMAND, only for
    /// the one entry whose hash actually matches. `None` until the first
    /// lookup attempt on this thread; `Some(vec![])` once confirmed absent.
    static SEEDED_PROGRAM_CACHE_RAW: RefCell<Option<Vec<(u64, Vec<u8>)>>> =
        const { RefCell::new(None) };
}

/// Record that `program_hash` was compiled once; returns `true` when this is
/// at least the second compile (i.e. the result is worth storing).
fn program_cache_should_store(program_hash: u64) -> bool {
    PROGRAM_CACHE_SEEN.with(|seen| !seen.borrow_mut().insert(program_hash))
}

/// Look up `program_hash` among the build-time-embedded seeded entries
/// (Issue #10120), decoding ONLY that one entry's `CompiledProgram` on a
/// match. The raw `(hash, bytes)` list itself is loaded and cached (as
/// undecoded bytes) at most once per thread; a decoded hit is also inserted
/// into `PROGRAM_CACHE` so a repeat compile of the same program in this
/// process hits the normal path directly.
///
/// A postcard decode always yields `compile_context: None` (`#[serde(skip)]`),
/// so a decoded hit is passed through `restore_compile_context_from_program`
/// with the caller's live `Program` — the same restore entry point the
/// Base-cache and `.sjvmbc` load paths use — so a seeded hit carries the same
/// compile context a fresh compile of the identical source would
/// (cache-restore parity invariant, Issues #10265/#10335).
fn seeded_program_cache_lookup(program_hash: u64, program: &Program) -> Option<CompiledProgram> {
    let raw_bytes = SEEDED_PROGRAM_CACHE_RAW.with(|slot| {
        let mut slot = slot.borrow_mut();
        if slot.is_none() {
            *slot = Some(super::seeded_cache::load_embedded_seeded_entries());
        }
        slot.as_ref()
            .and_then(|entries| entries.iter().find(|(hash, _)| *hash == program_hash))
            .map(|(_, bytes)| bytes.clone())
    })?;

    match super::seeded_cache::decode_seeded_compiled_program(&raw_bytes) {
        Ok(mut compiled) => {
            log_cache("[SeededCache] HIT - decoded a build-time-seeded CompiledProgram");
            restore_compile_context_from_program(&mut compiled, program);
            PROGRAM_CACHE.with(|cache| {
                cache.borrow_mut().insert(program_hash, compiled.clone());
            });
            Some(compiled)
        }
        Err(e) => {
            log_cache(&format!("[SeededCache] decode failed, ignoring entry: {e}"));
            None
        }
    }
}

fn cached_base_from_serialized(
    cache: super::precompile::SerializedBaseCache,
    source: &str,
) -> CachedBase {
    log_cache(&format!(
        "[Base Cache] Using {source} precompiled Base cache"
    ));

    super::profile::time("cache.replay_promotion_rules", || {
        for (t1, t2, ret) in &cache.promotion_rules {
            super::promotion::register_promotion_rule(t1, t2, ret);
        }
        super::promotion::mark_registry_initialized();
    });

    let mut compiled = cache.compiled;
    super::profile::time("cache.restore_base_compile_context", || {
        restore_base_compile_context(&mut compiled);
    });

    CachedBase {
        compiled,
        // Issue #9197 S7: the cache boundary now carries the typed
        // `MethodTableKey`; unwrap it back to the `String` the in-memory
        // dispatch/inference tables still key on (the full registry-persistence
        // conversion is #9199-gated — see docs/vm/TYPE_INTERNING.md S7).
        method_tables: cache
            .method_tables
            .into_iter()
            .map(|(key, table)| (key.into_string(), table))
            .collect(),
        closure_captures: cache.closure_captures,
        // Store rules in CachedBase so get_or_init_base_cache can replay on cache hits (Issue #3036)
        promotion_rules: cache.promotion_rules,
        // Persistent/embedded caches are serialized without inference snapshots
        // by current code. Drop non-empty legacy payloads that decode under the
        // same CACHE_VERSION so stale method-world assumptions do not seed user
        // compiles (Issue #6348, Issue #6495 follow-up).
        inference_results: Vec::new(),
    }
}

pub fn restore_compile_context_from_program(compiled: &mut CompiledProgram, program: &Program) {
    if compiled.compile_context.is_some() {
        return;
    }
    let module_imported_bindings = super::pipeline_ctx::collect_live_import_bindings(program);
    if !program_needs_restored_compile_context(compiled, program)
        && module_imported_bindings.is_empty()
    {
        return;
    }

    // Module-local abstract-type names, so module parametric structs can
    // qualify their `parent_type` exactly like the fresh pipeline does
    // (Issue #10337; the fresh side is `qualify_module_local_parent_type` in
    // `build_struct_tables`).
    let mut module_abstract_names: HashMap<String, HashSet<String>> = HashMap::new();
    for module in &program.modules {
        super::collect::collect_module_abstract_names(module, "", &mut module_abstract_names);
    }

    let mut parametric_structs = HashMap::new();
    let mut base_parametric_structs = HashMap::new();
    for def in &program.structs {
        if !def.type_params.is_empty() {
            parametric_structs.insert(
                def.name.clone(),
                super::ParametricStructDef { def: def.clone() },
            );
            if def.is_base_origin {
                base_parametric_structs.insert(
                    def.name.clone(),
                    super::ParametricStructDef { def: def.clone() },
                );
            }
        }
    }
    for module in &program.modules {
        register_restored_module_parametric_structs(
            &mut parametric_structs,
            module,
            "",
            &module_abstract_names,
        );
    }

    // Recover the inner-constructor flag from the IR (top-level AND module
    // structs, with `Foo{Int64}` resolved through `Foo`), so cache-restored
    // struct tables do not resurrect the field-count default constructor for
    // structs whose inner constructors suppress it (Issue #10092).
    let inner_ctor_flags = super::collect::collect_inner_constructor_flags(
        program.structs.iter(),
        program.modules.iter(),
    );

    // Issue #10988/#11078: rebuild the module registry by walking this
    // (possibly cache-restored) program's module tree in the SAME deterministic
    // depth-first order `register_module_ids` uses on the fresh-compile path,
    // and seed the struct registry with it BEFORE any struct is registered.
    // The owner `ModuleId` behind every `StructId` then comes from the module
    // tree, not from the order this lane happens to register structs in — which
    // differs from the fresh lane's (this one seeds every cached `struct_defs`
    // entry, parametric instantiations included, up front). That is what makes
    // a cache-restored session's `StructId`s agree with a fresh compile's:
    // "derive, don't reinterpret a persisted counter"
    // (`docs/vm/CACHE_ARCHITECTURE.md`, owner-scoped id relocation Pattern A).
    let mut module_registry = super::ModuleInternTable::new();
    for module in &program.modules {
        super::collect::register_module_ids(module, "", &mut module_registry);
    }

    let mut struct_table = super::StructRegistry::with_modules(module_registry.clone());
    for (type_id, def) in compiled.struct_defs.iter().enumerate() {
        let owner = restored_struct_owner(&def.name, &parametric_structs);
        struct_table.insert_owned(
            def.name.clone(),
            &owner,
            super::StructInfo {
                type_id,
                is_mutable: def.is_mutable,
                fields: def.fields.clone(),
                has_inner_constructor: super::collect::inner_constructor_flag_for(
                    &inner_ctor_flags,
                    &def.name,
                ),
            },
        );
    }

    // Mirror the fresh pipeline's bare-name aliasing for concrete module
    // structs (Issue #10337): `build_struct_tables` registers a module struct
    // under BOTH its qualified name (`M.Name`, the `struct_defs` entry above)
    // and its bare name (`Name`), with later modules winning (the clobber
    // ordering Issue #10078 documents). Rebuilding the table from
    // `struct_defs` alone loses every bare alias — e.g. the restored bare
    // `SpinLock` pointed at the top-level def instead of `Threads.SpinLock`.
    // IR module order matches the fresh pass's `all_structs` order
    // (top-level structs first, then modules depth-first).
    let mut module_structs = Vec::new();
    for module in &program.modules {
        super::collect::collect_module_structs(module, "", &mut module_structs);
    }
    for (def, module_path) in &module_structs {
        if def.type_params.is_empty() {
            let qualified = format!("{}.{}", module_path, def.name);
            let _ = struct_table.insert_alias(def.name.clone(), &qualified);
        }
    }
    let struct_table = struct_table;

    // Issue #10334: fresh compilation decides these gates from resolved method
    // tables. Replaying the finalized decision is exact for module-owned methods
    // and alias receivers; source-IR re-derivation was not.
    let specialization_disable_flags = compiled.specialization_disable_flags;

    let mut type_aliases = HashMap::new();
    // Mirror the fresh pipeline (Issue #5065): Base const aliases (e.g.
    // `ComplexF64`) live in the prelude program and are NOT merged into a user
    // program's `type_aliases` by `merge_prelude_into_user_program`, so
    // register them FIRST and let program/module aliases override (later
    // definition wins, matching upstream). Without this the context rebuilt
    // on the `.sjvmbc` path silently lost every Base alias (Issue #10336).
    // When `program` IS the prelude (`restore_base_compile_context`), this
    // re-registers identical entries and is a no-op.
    if let Some(prelude) = crate::get_prelude_program() {
        for alias in &prelude.type_aliases {
            register_restored_type_alias(&mut type_aliases, alias);
        }
    }
    for alias in &program.type_aliases {
        register_restored_type_alias(&mut type_aliases, alias);
    }
    for module in &program.modules {
        register_restored_module_type_aliases(&mut type_aliases, module, "");
    }
    let mut module_base_exports_visibility = HashMap::new();
    let mut module_implicit_standard_bindings = HashMap::new();
    for module in &program.modules {
        super::collect::collect_module_base_exports_visibility(
            module,
            "",
            &mut module_base_exports_visibility,
        );
        super::collect::collect_module_implicit_standard_bindings(
            module,
            "",
            &mut module_implicit_standard_bindings,
        );
    }
    let base_exported_names = crate::julia::base::exported_names()
        .iter()
        .cloned()
        .collect();

    compiled.compile_context = Some(RuntimeCompileContext {
        struct_table,
        struct_defs: compiled.struct_defs.clone(),
        parametric_structs,
        base_parametric_structs,
        type_aliases,
        module_imported_bindings,
        module_base_exports_visibility,
        module_implicit_standard_bindings,
        base_exported_names,
        inference_global_types: compiled
            .inference_global_types_snapshot
            .iter()
            .cloned()
            .collect(),
        // User primitive types from the (deserialized) compiled program, so the
        // reconstructed context keeps them visible to type reflection (Issue #5058).
        primitive_types: compiled.primitive_types.clone(),
        disable_array_getindex_specialization: specialization_disable_flags.array_getindex,
        disable_array_setindex_specialization: specialization_disable_flags.array_setindex,
        disable_field_access_specialization: specialization_disable_flags.field_access,
        module_registry,
    });
}

/// Rebuild the declaration owner that is intentionally not serialized with a
/// concrete struct's display spelling (Issue #11046). Qualified spellings are
/// self-describing. A bare parametric spelling belongs to the unique qualified
/// family reconstructed from the source IR; ambiguous families already retain
/// a qualified concrete spelling in the fresh pipeline.
fn restored_struct_owner(
    concrete_name: &str,
    parametric_structs: &HashMap<String, super::ParametricStructDef>,
) -> String {
    let base_end = concrete_name.find('{').unwrap_or(concrete_name.len());
    if let Some(dot) = concrete_name[..base_end].rfind('.') {
        return concrete_name[..dot].to_string();
    }

    let Some(brace) = concrete_name.find('{') else {
        return "Main".to_string();
    };
    let base = &concrete_name[..brace];
    let suffix = format!(".{base}");
    let mut owners = parametric_structs
        .keys()
        .filter_map(|name| name.strip_suffix(&suffix));
    match (owners.next(), owners.next()) {
        (Some(owner), None) => owner.to_string(),
        _ => "Main".to_string(),
    }
}

fn program_needs_restored_compile_context(compiled: &CompiledProgram, program: &Program) -> bool {
    !compiled.specializable_functions.is_empty()
        || !compiled.primitive_types.is_empty()
        || !program.primitive_types.is_empty()
        || !program.type_aliases.is_empty()
        || !program.modules.is_empty()
        || program.usings.iter().any(using_needs_live_binding_context)
        || program
            .structs
            .iter()
            .any(|def| !def.type_params.is_empty())
        || program
            .modules
            .iter()
            .any(module_needs_restored_compile_context)
}

fn module_needs_restored_compile_context(module: &Module) -> bool {
    !module.type_aliases.is_empty()
        || module.usings.iter().any(using_needs_live_binding_context)
        || !module.primitive_types.is_empty()
        || module.structs.iter().any(|def| !def.type_params.is_empty())
        || module
            .submodules
            .iter()
            .any(module_needs_restored_compile_context)
}

fn using_needs_live_binding_context(using: &crate::ir::core::UsingImport) -> bool {
    using.symbols.is_some() || !using.alias_bindings.is_empty()
}

fn restored_type_alias_runtime_target(alias: &TypeAliasDef) -> String {
    if alias.params.is_empty() {
        alias.target_type.clone()
    } else {
        match alias.target_type.split_once('{') {
            Some((base, _)) => base.trim().to_string(),
            None => alias.target_type.clone(),
        }
    }
}

fn register_restored_type_alias(type_aliases: &mut HashMap<String, String>, alias: &TypeAliasDef) {
    type_aliases.insert(
        alias.name.clone(),
        restored_type_alias_runtime_target(alias),
    );
}

fn register_restored_module_type_aliases(
    type_aliases: &mut HashMap<String, String>,
    module: &Module,
    prefix: &str,
) {
    let module_path = if prefix.is_empty() {
        module.name.clone()
    } else {
        format!("{}.{}", prefix, module.name)
    };

    for alias in &module.type_aliases {
        let target = restored_type_alias_runtime_target(alias);
        type_aliases.insert(format!("{}.{}", module_path, alias.name), target);
    }

    for submodule in &module.submodules {
        register_restored_module_type_aliases(type_aliases, submodule, &module_path);
    }
}

fn register_restored_module_parametric_structs(
    parametric_structs: &mut HashMap<String, super::ParametricStructDef>,
    module: &Module,
    prefix: &str,
    module_abstract_names: &HashMap<String, HashSet<String>>,
) {
    let module_path = if prefix.is_empty() {
        module.name.clone()
    } else {
        format!("{}.{}", prefix, module.name)
    };

    for def in &module.structs {
        if def.type_params.is_empty() {
            continue;
        }
        // Mirror the fresh pipeline exactly (Issue #10337): the stored def
        // carries the module-qualified parent type, and the struct is
        // registered under BOTH the qualified and the bare name (the bare
        // alias is what lets `Point(...)` resolve after `using .MyGeometry`;
        // later modules win, as in `build_struct_tables`).
        let mut stored_def = def.clone();
        stored_def.parent_type = super::collect::qualify_module_local_parent_type(
            def.parent_type.clone(),
            &module_path,
            module_abstract_names,
        );
        parametric_structs.insert(
            format!("{}.{}", module_path, def.name),
            super::ParametricStructDef {
                def: stored_def.clone(),
            },
        );
        parametric_structs.insert(
            def.name.clone(),
            super::ParametricStructDef { def: stored_def },
        );
    }

    for submodule in &module.submodules {
        register_restored_module_parametric_structs(
            parametric_structs,
            submodule,
            &module_path,
            module_abstract_names,
        );
    }
}

fn restore_base_compile_context(compiled: &mut CompiledProgram) {
    let Some(prelude) = crate::get_prelude_program() else {
        return;
    };
    restore_compile_context_from_program(compiled, prelude);
}

fn env_flag_is_enabled(value: Option<&str>) -> bool {
    matches!(value, Some("1"))
}

fn persistent_base_cache_disabled() -> bool {
    env_flag_is_enabled(
        env::var("SUBSET_JULIA_VM_DISABLE_PERSISTENT_BASE_CACHE")
            .ok()
            .as_deref(),
    )
}

fn workspace_target_dir() -> PathBuf {
    if let Ok(target_dir) = env::var("CARGO_TARGET_DIR") {
        return PathBuf::from(target_dir);
    }

    Path::new(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .map(|p| p.join("target"))
        .unwrap_or_else(|| PathBuf::from("target"))
}

/// Persistent Base cache namespace.
///
/// Keep this separate from the serialized `CACHE_VERSION`: Issue #6495 keeps the
/// wire format/version stable while deleting the in-memory `MethodSig`
/// projections, but old local persistent caches may still contain Base bytecode
/// compiled under the pre-deletion dispatch/inference code. A path namespace
/// split makes those generated artifacts miss without changing the wire schema.
const PERSISTENT_BASE_CACHE_NAMESPACE: &str = "v3";

fn persistent_base_cache_path() -> PathBuf {
    let hash = super::precompile::compute_base_cache_hash();
    workspace_target_dir().join(format!(
        "sjulia_base_cache_{PERSISTENT_BASE_CACHE_NAMESPACE}_{hash}.bin"
    ))
}

#[derive(Debug, Clone)]
pub struct CacheArtifactDebugStatus {
    pub state: &'static str,
    pub path: Option<PathBuf>,
    pub detail: Option<String>,
}

#[derive(Debug, Clone)]
pub struct BaseCacheFingerprints {
    pub cache_hash: String,
    pub schema_fingerprint: String,
    pub compiler_build_fingerprint: String,
    pub enum_variant_fingerprint: String,
}

#[derive(Debug, Clone)]
pub struct BaseCacheDebugStatus {
    pub load_source: &'static str,
    pub compile_cache_disabled: bool,
    pub persistent_disabled: bool,
    pub embedded: CacheArtifactDebugStatus,
    pub persistent: CacheArtifactDebugStatus,
    pub fingerprints: BaseCacheFingerprints,
}

fn validate_base_cache_artifact(
    path: Option<PathBuf>,
    bytes: Option<&[u8]>,
) -> CacheArtifactDebugStatus {
    let Some(bytes) = bytes else {
        return CacheArtifactDebugStatus {
            state: "missing",
            path,
            detail: None,
        };
    };

    match super::precompile::deserialize_base_cache(bytes) {
        Ok(_) => CacheArtifactDebugStatus {
            state: "valid",
            path,
            detail: None,
        },
        Err(e) => CacheArtifactDebugStatus {
            state: "invalid",
            path,
            detail: Some(e),
        },
    }
}

fn read_base_cache_artifact_without_side_effects(path: &Path) -> CacheArtifactDebugStatus {
    match fs::read(path) {
        Ok(bytes) => validate_base_cache_artifact(Some(path.to_path_buf()), Some(&bytes)),
        Err(e) if e.kind() == ErrorKind::NotFound => CacheArtifactDebugStatus {
            state: "missing",
            path: Some(path.to_path_buf()),
            detail: None,
        },
        Err(e) => CacheArtifactDebugStatus {
            state: "invalid",
            path: Some(path.to_path_buf()),
            detail: Some(format!("read failed: {e}")),
        },
    }
}

/// Report how Base bytecode cache loading would resolve without deleting stale
/// persistent files or compiling from source (Issue #8718).
pub fn base_cache_debug_status() -> BaseCacheDebugStatus {
    let compile_cache_disabled = is_cache_disabled();
    let persistent_disabled = persistent_base_cache_disabled();
    let embedded =
        validate_base_cache_artifact(None, super::embedded_cache::embedded_cache_bytes());
    let persistent_path = persistent_base_cache_path();
    let persistent = if persistent_disabled {
        CacheArtifactDebugStatus {
            state: "disabled",
            path: Some(persistent_path),
            detail: None,
        }
    } else {
        read_base_cache_artifact_without_side_effects(&persistent_path)
    };
    let load_source = if compile_cache_disabled {
        "none"
    } else if embedded.state == "valid" {
        "embedded"
    } else if persistent.state == "valid" {
        "persistent"
    } else {
        "none"
    };

    BaseCacheDebugStatus {
        load_source,
        compile_cache_disabled,
        persistent_disabled,
        embedded,
        persistent,
        fingerprints: BaseCacheFingerprints {
            cache_hash: super::precompile::compute_base_cache_hash(),
            schema_fingerprint: super::precompile::base_cache_schema_fingerprint(),
            compiler_build_fingerprint: super::precompile::compiler_build_fingerprint().to_string(),
            enum_variant_fingerprint: super::precompile::enum_variant_fingerprint(),
        },
    }
}

fn read_persistent_base_cache(path: &Path) -> Option<CachedBase> {
    let bytes = match fs::read(path) {
        Ok(bytes) => bytes,
        Err(e) if e.kind() == ErrorKind::NotFound => return None,
        Err(e) => {
            log_cache(&format!(
                "[Base Cache] Persistent cache read failed at {}: {e}",
                path.display()
            ));
            return None;
        }
    };

    match super::precompile::deserialize_base_cache(&bytes) {
        Ok(cache) => Some(cached_base_from_serialized(cache, "persistent")),
        Err(e) => {
            log_cache(&format!(
                "[Base Cache] Ignoring stale persistent cache at {}: {e}",
                path.display()
            ));
            let _ = fs::remove_file(path);
            None
        }
    }
}

fn acquire_persistent_cache_lock(cache_path: &Path) -> Option<PersistentCacheLock> {
    let lock_path = cache_path.with_extension("lock");
    let stale_after = Duration::from_secs(20 * 60);

    for _ in 0..1200 {
        match fs::OpenOptions::new()
            .write(true)
            .create_new(true)
            .open(&lock_path)
        {
            Ok(_) => {
                return Some(PersistentCacheLock { path: lock_path });
            }
            Err(e) if e.kind() == ErrorKind::AlreadyExists => {
                if let Ok(metadata) = fs::metadata(&lock_path) {
                    let is_stale = metadata
                        .modified()
                        .ok()
                        .and_then(|modified| SystemTime::now().duration_since(modified).ok())
                        .is_some_and(|age| age > stale_after);
                    if is_stale {
                        let _ = fs::remove_file(&lock_path);
                    }
                }
                if cache_path.exists() {
                    return None;
                }
                std::thread::sleep(Duration::from_millis(50));
            }
            Err(e) => {
                log_cache(&format!(
                    "[Base Cache] Persistent cache lock failed at {}: {e}",
                    lock_path.display()
                ));
                return None;
            }
        }
    }

    log_cache("[Base Cache] Timed out waiting for persistent cache lock");
    None
}

fn write_persistent_base_cache(path: &Path, cache: &CachedBase) {
    let Some(parent) = path.parent() else {
        return;
    };
    if let Err(e) = fs::create_dir_all(parent) {
        log_cache(&format!(
            "[Base Cache] Failed to create persistent cache directory {}: {e}",
            parent.display()
        ));
        return;
    }

    let bytes = match super::precompile::serialize_base_cache(
        &cache.compiled,
        &cache.method_tables,
        &cache.closure_captures,
        &cache.inference_results,
    ) {
        Ok(bytes) => bytes,
        Err(e) => {
            log_cache(&format!(
                "[Base Cache] Persistent cache serialize failed: {e}"
            ));
            return;
        }
    };

    let tmp_path = path.with_extension(format!("tmp.{}", std::process::id()));
    if let Err(e) = fs::write(&tmp_path, bytes) {
        log_cache(&format!(
            "[Base Cache] Persistent cache write failed at {}: {e}",
            tmp_path.display()
        ));
        return;
    }
    if let Err(e) = fs::rename(&tmp_path, path) {
        log_cache(&format!(
            "[Base Cache] Persistent cache rename failed from {} to {}: {e}",
            tmp_path.display(),
            path.display()
        ));
        let _ = fs::remove_file(&tmp_path);
        return;
    }

    log_cache(&format!(
        "[Base Cache] Wrote persistent precompiled Base cache to {}",
        path.display()
    ));
}

fn compile_base_functions_from_source() -> CResult<CachedBase> {
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
        primitive_types: prelude.primitive_types.clone(),
        type_aliases: prelude.type_aliases.clone(),
        main: prelude.main.clone(),
        modules: prelude.modules.clone(),
        usings: vec![],
        macros: vec![],
        enums: vec![],
        base_function_count: prelude.functions.len(),
    };

    // Compile Base functions and capture method tables + closure captures
    let super::pipeline_ctx::CoreCompileOutput {
        compiled,
        method_tables,
        closure_captures,
        inference_results,
        ..
    } = crate::compile::compile_core_program_internal(
        &base_program,
        &std::collections::HashMap::new(),
        &std::collections::HashMap::new(),
        crate::compile::CompilerCacheInput::default(),
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
        inference_results,
    })
}

/// Compile only the Base functions from the prelude with method tables.
fn compile_base_functions() -> CResult<CachedBase> {
    // Try embedded cache first (build-time precompiled, Issue #2929).
    if let Some(embedded) = super::embedded_cache::load_embedded_cache() {
        return Ok(cached_base_from_serialized(embedded, "embedded"));
    }

    if persistent_base_cache_disabled() {
        return compile_base_functions_from_source();
    }

    let cache_path = persistent_base_cache_path();
    if let Some(cached) = read_persistent_base_cache(&cache_path) {
        return Ok(cached);
    }

    let Some(_lock) = acquire_persistent_cache_lock(&cache_path) else {
        if let Some(cached) = read_persistent_base_cache(&cache_path) {
            return Ok(cached);
        }
        return compile_base_functions_from_source();
    };

    if let Some(cached) = read_persistent_base_cache(&cache_path) {
        return Ok(cached);
    }

    let compiled = compile_base_functions_from_source()?;
    write_persistent_base_cache(&cache_path, &compiled);
    Ok(compiled)
}

/// Extract the return type name from a `promote_rule` function body.
///
/// The type inference engine infers `promote_rule` return types as `ValueType::Any`
/// because functions return type objects (e.g., `Int64` in return position), not values.
/// This function bypasses inference by reading the type name directly from the IR body.
///
/// Two body patterns are recognised:
/// - Primitive type: `Stmt::Expr { expr: Var("Int64") }` → `"Int64"`
/// - Struct type: `Stmt::Expr { expr: Literal::DataType("Complex{Float64}") }` → `"Complex{Float64}"`
fn extract_return_type_from_promote_rule_body(body: &crate::ir::core::Block) -> Option<String> {
    use crate::ir::core::{BuiltinOp, Expr, Literal, Stmt};

    // promote_rule bodies are a single expression statement
    if body.stmts.len() != 1 {
        return None;
    }

    match &body.stmts[0] {
        Stmt::Expr { expr, .. } => match expr {
            // Primitive type return: `Int64`, `Float64`, etc.
            Expr::Var(name, _) => Some(name.to_string()),
            // Struct type return: `Builtin { TypeOf, [Literal(Str("Complex{Float64}"))] }`
            // This is the legacy representation for parametric struct type objects.
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
            Expr::Literal(Literal::DataType(type_name), _) => Some(type_name.clone()),
            Expr::DynamicTypeConstruct {
                base,
                base_expr,
                type_args,
                splat_mask,
                ..
            } => static_dynamic_type_construct_name(
                base,
                base_expr.as_deref(),
                type_args,
                splat_mask,
            ),
            _ => None,
        },
        _ => None,
    }
}

fn type_name_from_type_value_expr(expr: &crate::ir::core::Expr) -> Option<String> {
    use crate::ir::core::{BuiltinOp, Expr, Literal};

    match expr {
        Expr::Var(name, _) => Some(name.to_string()),
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
        Expr::Literal(Literal::DataType(type_name), _) => Some(type_name.clone()),
        Expr::Literal(Literal::Int(value), _) => Some(value.to_string()),
        Expr::Literal(Literal::Int128(value), _) => Some(value.to_string()),
        Expr::Literal(Literal::Str(value), _) => Some(value.clone()),
        Expr::Literal(Literal::Symbol(value), _) => Some(format!(":{value}")),
        Expr::DynamicTypeConstruct {
            base,
            base_expr,
            type_args,
            splat_mask,
            ..
        } => static_dynamic_type_construct_name(base, base_expr.as_deref(), type_args, splat_mask),
        _ => None,
    }
}

fn static_dynamic_type_construct_name(
    base: &str,
    base_expr: Option<&crate::ir::core::Expr>,
    type_args: &[crate::ir::core::Expr],
    splat_mask: &[bool],
) -> Option<String> {
    use crate::ir::core::Expr;

    if !splat_mask.is_empty() {
        return None;
    }

    match base_expr {
        None => {}
        Some(Expr::Var(name, _)) if name == base => {}
        Some(_) => return None,
    }

    type_args
        .iter()
        .map(type_name_from_type_value_expr)
        .collect::<Option<Vec<_>>>()
        .map(|args| format!("{base}{{{}}}", args.join(",")))
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
fn extract_promotion_rules_from_ir(functions: &[std::sync::Arc<crate::ir::core::Function>]) {
    let mut count = 0;
    for func in functions {
        if func.name != "promote_rule" || func.params.len() != 2 {
            continue;
        }

        let p0_type = func.params[0].effective_type();
        let p1_type = func.params[1].effective_type();

        // Skip generic promote_rule(::Type{T}, ::Type{S}) — these return Union{} / Bottom.
        // Methods with a type variable in either Type{} slot (including bounded
        // forms like Type{<:Integer} or the parametric Complex/Rational rules)
        // cannot be flattened to concrete (t1, t2) pairs, so they are left to the
        // Rust promote_type fallback / runtime Julia dispatch.
        if is_typeof_with_type_var(&p0_type) || is_typeof_with_type_var(&p1_type) {
            continue;
        }

        // A parameter may be a single `Type{X}` or a Union of them
        // (`Union{Type{Int8}, Type{UInt8}}`, as in julia/base/int.jl:775-788 and
        // the Rational/Big rules added for Issue #5070). Expand the cross-product
        // so every concrete pair is registered with the rule's return type.
        let types1 = extract_concrete_typeof_names(&p0_type);
        let types2 = extract_concrete_typeof_names(&p1_type);
        let return_type = extract_return_type_from_promote_rule_body(&func.body);

        if let Some(ret) = return_type {
            if !types1.is_empty() && !types2.is_empty() {
                for t1 in &types1 {
                    for t2 in &types2 {
                        super::promotion::register_promotion_rule(t1, t2, &ret);
                        count += 1;
                        log_cache(&format!(
                            "[Promotion] Registered: promote_rule({}, {}) = {}",
                            t1, t2, ret
                        ));
                    }
                }
            }
        }
    }
    log_cache(&format!(
        "[Promotion] Registered {} promotion rules from Julia IR (Issue #3025)",
        count
    ));

    // Mark registry as initialized
    super::promotion::mark_registry_initialized();
}

/// Extract the concrete type names from a `promote_rule` parameter.
///
/// Handles both a single `Type{X}` (returns `[X]`) and a `Union{Type{A},
/// Type{B}, ...}` of concrete `Type{}` entries (returns `[A, B, ...]`).
/// A Union member that is not a concrete `Type{}` (e.g. contains a type
/// variable) makes the whole parameter non-concrete and yields an empty Vec so
/// the caller skips the rule (Issue #5070).
fn extract_concrete_typeof_names(ty: &crate::types::JuliaType) -> Vec<String> {
    use crate::types::JuliaType;
    match ty {
        JuliaType::TypeOf(inner) => match inner.as_ref() {
            JuliaType::Union(_) => extract_concrete_typeof_names(inner),
            JuliaType::Struct(name) if name.starts_with("Union{") => {
                if let Some(parsed) = JuliaType::from_name(name) {
                    extract_concrete_typeof_names(&parsed)
                } else {
                    Vec::new()
                }
            }
            _ => extract_type_from_typeof(ty).into_iter().collect(),
        },
        JuliaType::Union(members) => {
            let mut names = Vec::with_capacity(members.len());
            for m in members {
                if is_typeof_with_type_var(m) {
                    return Vec::new();
                }
                match extract_type_from_typeof(m) {
                    Some(name) => names.push(name),
                    None => return Vec::new(),
                }
            }
            names
        }
        _ => match extract_type_from_typeof(ty) {
            Some(name) => vec![name],
            None => Vec::new(),
        },
    }
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
    vt: &crate::bytecode::ValueType,
    struct_defs: &[StructDefInfo],
) -> Option<String> {
    use crate::bytecode::ValueType;

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
        // Nothing/Missing are concrete singleton DataTypes — NOT the bottom type
        // `Union{}`. Conflating `Nothing` with `Union{}` is the same bug fixed in
        // the runtime subtype path by Issue #5257; the singleton's name is just
        // "Nothing" (resp. "Missing"). (Issue #5069)
        ValueType::Nothing => Some("Nothing".to_string()),
        ValueType::Missing => Some("Missing".to_string()),
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
        let cached = Rc::clone(cache.as_ref().ok_or_else(|| {
            super::types::CompileError::Msg("Base cache unavailable".to_string())
        })?);

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
    PROGRAM_CACHE_SEEN.with(|seen| seen.borrow_mut().clear());
    // Clear the promotion registry together with BASE_CACHE to maintain
    // the invariant that registries and cache are always in sync (Issue #3038).
    super::promotion::clear_registry();
}

/// Clear program-level caches and promotion registry WITHOUT clearing Base cache.
///
/// Fixture test chunks run 32 fixtures per process; the first fixture compiles
/// Base and subsequent fixtures should reuse the thread-local `BASE_CACHE`.
/// `clear_cache()` defeats that reuse, so the fixture harness calls this instead
/// to preserve the expensive Base compilation across fixtures within a chunk
/// while still isolating per-fixture program caches and promotion state
/// (Issue #9843).
pub fn clear_non_base_cache() {
    PROGRAM_CACHE.with(|cache| cache.borrow_mut().clear());
    PROGRAM_CACHE_SEEN.with(|seen| seen.borrow_mut().clear());
    super::promotion::clear_registry();
}

/// Clear only the per-program compiled-program cache.
///
/// Parity harnesses that compile the same source through alternate compiler
/// gates must prevent a full-program cache hit from reusing the first gate's
/// output under the second gate, but they do not need to drop Base or replay
/// Base-derived registries between the two runs (Issue #9865).
pub fn clear_program_cache() {
    PROGRAM_CACHE.with(|cache| cache.borrow_mut().clear());
    PROGRAM_CACHE_SEEN.with(|seen| seen.borrow_mut().clear());
}

/// Export the current Base cache for serialization (used by --precompile-base).
/// Returns None if cache is not initialized.
pub(crate) fn export_base_cache() -> Option<(
    CompiledProgram,
    HashMap<String, super::MethodTable>,
    HashMap<String, std::collections::HashSet<String>>,
    Vec<(InferenceCacheKey, CachedReturn)>,
)> {
    BASE_CACHE.with(|cache| {
        cache.borrow().as_ref().map(|c| {
            (
                c.compiled.clone(),
                c.method_tables.clone(),
                c.closure_captures.clone(),
                c.inference_results.clone(),
            )
        })
    })
}

/// Compute a hash of the program for caching
/// This creates a fingerprint based on the program structure
///
/// `pub(super)` so `seeded_cache.rs` (Issue #10120) can compute the SAME hash
/// at build time for its fixed list of seed programs, keyed identically to
/// how a real one-shot CLI compile hashes them at runtime.
pub(super) fn compute_program_hash(
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
    global_type_entries.sort_by_key(|(a, _)| *a);
    for (name, ty) in global_type_entries {
        name.hash(&mut hasher);
        format!("{:?}", ty).hash(&mut hasher);
    }

    // Hash struct-name context used to resolve stable struct IDs in REPL
    let mut global_struct_entries: Vec<_> = global_struct_names.iter().collect();
    global_struct_entries.sort_by_key(|(a, _)| *a);
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
    super::profile::reset();

    // If cache is disabled, use regular compilation (for development/testing)
    if is_cache_disabled() {
        log_cache("[Cache] DISABLED via SUBSET_JULIA_VM_DISABLE_CACHE");
        let result = super::profile::time("cache.disabled_compile_core_program", || {
            super::compile_core_program_with_globals(program, global_types, global_struct_names)
        });
        super::profile::print_summary("compile_with_cache");
        return result;
    }

    // Option C: Check full program cache first
    let program_hash = super::profile::time("cache.compute_program_hash", || {
        compute_program_hash(program, global_types, global_struct_names)
    });
    let cached_program = super::profile::time("cache.full_program_lookup", || {
        PROGRAM_CACHE.with(|cache| cache.borrow().get(&program_hash).cloned())
    });

    // Fall back to any build-time-precompiled common short program (Issue
    // #10120) BEFORE paying a full compile, so a matching program hits a
    // cache on its FIRST compile in this process, not just its second
    // (unlike a normal compile result, a seeded entry costs nothing to
    // produce at runtime -- decoding it is the only cost -- so Issue #6348's
    // one-shot-CLI store-on-second-compile tradeoff doesn't apply to it).
    // Decodes ON DEMAND (only a hash match pays a decode), not eagerly for
    // every embedded seed -- see `seeded_program_cache_lookup`'s doc comment.
    let cached_program = cached_program.or_else(|| {
        super::profile::time("cache.seeded_program_lookup", || {
            seeded_program_cache_lookup(program_hash, program)
        })
    });

    if let Some(compiled) = cached_program {
        log_cache("[Cache] FULL HIT - reusing entire compiled program");
        super::profile::print_summary("compile_with_cache");
        return Ok(compiled);
    }

    // Issue #2726 / #2790: Base cache requires exact function-order alignment.
    // If prelude methods were replaced by user exact-signature definitions,
    // `base_function_count` shrinks and cached indices become invalid.
    let prelude_function_count = super::profile::time("cache.get_prelude_function_count", || {
        crate::get_prelude_program()
            .map(|p| p.functions.len())
            .unwrap_or(program.base_function_count)
    });
    let skip_base_cache = super::profile::time("cache.should_skip_base_cache", || {
        should_skip_base_cache_for_program(program, prelude_function_count)
    });
    if skip_base_cache {
        log_cache(
            "[Cache] Base cache BYPASS - user program changes Base dispatch visibility; using full compile path",
        );
        let compiled = super::profile::time("cache.bypass_compile_core_program", || {
            super::compile_core_program_with_globals(program, global_types, global_struct_names)
        })?;
        super::profile::time("cache.program_cache_store", || {
            if program_cache_should_store(program_hash) {
                PROGRAM_CACHE.with(|cache| {
                    cache.borrow_mut().insert(program_hash, compiled.clone());
                });
            }
        });
        super::profile::print_summary("compile_with_cache");
        return Ok(compiled);
    }

    // Cache miss - proceed with Base caching (Option A + Base bytecode caching)
    let cache_was_initialized =
        super::profile::time("cache.is_base_cache_initialized", is_cache_initialized);
    let base_cache = super::profile::time("cache.get_or_init_base_cache", get_or_init_base_cache)?;

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

    // Preloaded-package bytecode cache (Issue #9189): `None` with zero
    // filesystem/compile work whenever `preload_cache::PRELOAD_PACKAGES` is
    // empty. Held here so the borrow passed into
    // `CompilerCacheInput` below outlives the compile call.
    let preload_cache_handle = super::preload_cache::get_or_init_preload_cache();

    // Compile with precompiled Base bytecode AND cached method tables + closure captures (Option A!)
    let output = super::profile::time("cache.compile_core_program_internal", || {
        crate::compile::compile_core_program_internal(
            program,
            global_types,
            global_struct_names,
            crate::compile::CompilerCacheInput {
                current_input_type_names: None,
                current_input_runtime_nominal_names: None,
                precompiled_base: Some(&base_cache.compiled),
                method_tables: Some(&base_cache.method_tables),
                closure_captures: Some(&base_cache.closure_captures),
                inference_results: Some(&base_cache.inference_results),
                preload_cache: preload_cache_handle.as_ref().map(|c| &c.modules),
                preload_closure_layout: preload_cache_handle
                    .as_ref()
                    .map(|c| c.closure_layout.as_slice()),
                extra_imported_functions: None,
                global_slot_seed: None,
                extra_module_metadata: None,
                extra_inner_constructor_type_names: None,
                repl_current_function_count: None,
                repl_current_struct_count: None,
                repl_append_only_new_generics: false,
            },
        )
    })?;
    let compiled = output.compiled;

    log_cache(&format!(
        "[Cache] Compiled {} user functions + main",
        program.functions.len() - program.base_function_count
    ));

    // Store in full program cache for future use (only once the same program
    // is compiled a second time — see PROGRAM_CACHE_SEEN, Issue #6348).
    super::profile::time("cache.program_cache_store", || {
        if program_cache_should_store(program_hash) {
            PROGRAM_CACHE.with(|cache| {
                cache.borrow_mut().insert(program_hash, compiled.clone());
            });
        }
    });

    super::profile::print_summary("compile_with_cache");
    Ok(compiled)
}

// ===========================================================================
// REPL input-delta compilation (Issue #9199 S5).
//
// Under the persistent REPL model, `REPLSession::eval` compiles ONLY the new
// input against the accumulated program instead of re-lowering + re-compiling
// the whole session every eval. This makes per-eval COMPILE cost independent of
// session length (the epic's exit criterion — ADR §"Exit criteria").
//
// It reuses the EXACT precompiled-prefix machinery the Base cache already uses:
// the pipeline splits `functions[0..base_function_count]` (reused verbatim from
// `precompiled_base`) from the user segment (compiled fresh), keyed purely by
// index. We grow that reused prefix to include prior user definitions by
// carrying the previous eval's compiled output + its `merged` IR forward and
// appending only the new input, so the pipeline compiles just the delta.
//
// Correctness is preserved by construction: the prefix IR is the *identical*
// `merged` Program that produced `bundle`, so `precompiled_base.functions` and
// the extended IR align index-for-index. The REPL session gates which inputs
// take this path (append-only new-name functions + expressions); anything that
// could change prior dispatch (redefinition, base extension, structs/modules/
// macros/types, opaque `eval`, `using`) routes back through the full
// accumulate-and-recompile path, which refreshes this cache. The differential
// harness (`tests/repl_differential_9199_tests.rs`) proves Legacy ≡ Persistent.
// ===========================================================================

/// Accumulated compiled state carried across `Persistent` evals (Issue #9199 S5).
///
/// Holds the reusable compile bundle for the whole accumulated program
/// (`[base, generated, prior user defs]` compiled once) plus the set of all
/// function names bound in it. The next delta-safe eval reuses `bundle.compiled`
/// as the precompiled prefix and compiles ONLY its own new input against it, so
/// prior user functions are reused verbatim from the compiled prefix rather than
/// re-lowered and re-compiled.
///
/// The key alignment fact (learned the hard way): compilation appends generated
/// functions — inner constructors, lifted lambdas — AFTER the source functions,
/// so `bundle.compiled.functions` is `[base source | generated | user]`, NOT the
/// `[base | user]` shape of the IR. The delta therefore never tries to
/// reconstruct that layout in IR; it feeds the pipeline the ordinary
/// `merge_with_precompiled_base(input)` IR (`[base source | NEW user]`, whose
/// base prefix aligns index-for-index with `bundle.compiled`) and lets the
/// pipeline reuse the WHOLE `bundle.compiled` as the precompiled prefix and
/// append only the freshly-compiled new input — exactly how the Base cache reuse
/// already works, with the accumulated bundle standing in for the Base cache.
/// Module-surface metadata (function names, constant names, exports) for the
/// USER modules realized in the accumulated program (Issue #9199 LV5). Built
/// once per full recompile from the merged program's modules via
/// `collect_module_info`, and carried into the relocatable-delta compile so a
/// later delta that only REFERENCES a prior simple user module (`M.f()`,
/// `M.const`) resolves against the live VM's already-installed module functions
/// (in `bundle.method_tables` / the prefix) and its module-constant globals (in
/// frame-0), WITHOUT re-emitting the module body. Keyed by qualified module path
/// (`M`, `M.Sub`), exactly like the pipeline's own `module_functions` etc. Empty
/// for a session with no modules (the `Default`), so a non-module delta compile
/// is unaffected. Only module RESOLUTION metadata — never bodies — is carried;
/// the module's compiled function bodies already live verbatim in `bundle`.
#[derive(Debug, Default, Clone)]
pub(crate) struct ReplModuleMetadata {
    pub module_functions: HashMap<String, HashSet<String>>,
    pub module_exports: HashMap<String, HashSet<String>>,
    pub module_constants: HashMap<String, HashSet<String>>,
    pub module_publics: HashMap<String, Vec<String>>,
    /// Root module bindings realized in Main by the reused REPL prefix. The
    /// qualified surface maps alone prove that a module exists, but not that
    /// its root is lexically bound in Main (an unimported package may also be
    /// present in those maps).
    pub toplevel_module_bindings: HashSet<String>,
}

#[derive(Debug, Default, Clone)]
struct ReplMethodSourceSnapshot {
    methods: BTreeMap<ReplMethodIdentity, Function>,
    dependencies: BTreeMap<ReplMethodIdentity, BTreeSet<String>>,
}

#[derive(Debug, Default)]
struct ReplMethodRefreshPlan {
    methods: Vec<Arc<Function>>,
    refresh_ordinals_by_source: BTreeMap<usize, Vec<usize>>,
}

#[derive(Debug)]
struct ReplDefinitionTransaction {
    prior_function_names: HashSet<String>,
    prior_method_sources: ReplMethodSourceSnapshot,
    /// Only method tables touched by this source-bearing delta. `None` means
    /// the table did not exist before the transaction. Snapshotting the whole
    /// Base/user table map made every live source append O(total methods).
    prior_method_tables: HashMap<String, Option<MethodTable>>,
    /// Exact compiler method row for every source/refresh activation member.
    /// This retains an earlier same-signature row after the final table has
    /// replaced it with a later, possibly unreached definition.
    method_rows_by_index: HashMap<usize, (String, MethodSig)>,
    markerless_function_indices: HashSet<usize>,
    /// Julia-visible source methods keyed by their final aligned function index.
    /// Marker-less lowering helpers are deliberately absent.
    source_functions: Vec<(usize, Arc<Function>)>,
    initial_specializable_functions: Vec<SpecializableFunction>,
    specializable_updates: Vec<(usize, SpecializableFunction)>,
}

impl ReplMethodSourceSnapshot {
    fn from_functions<'a>(functions: impl IntoIterator<Item = &'a Arc<Function>>) -> Self {
        let mut snapshot = Self::default();
        snapshot.apply(functions);
        snapshot
    }

    fn apply<'a>(&mut self, functions: impl IntoIterator<Item = &'a Arc<Function>>) {
        for function in functions
            .into_iter()
            .filter(|function| !crate::compile::ir_inline::is_markerless_lowered_function(function))
        {
            let identity = ReplMethodIdentity::from_function(&function.name, function);
            let dependencies = super::ipo::call_graph::extract_called_functions(&function.body)
                .into_iter()
                .collect();
            self.methods
                .insert(identity.clone(), function.as_ref().clone());
            self.dependencies.insert(identity, dependencies);
        }
    }

    fn contains_generic(&self, name: &str) -> bool {
        self.methods
            .keys()
            .any(|identity| identity.function() == name)
    }

    fn refresh_plan_for(&self, input: &Program) -> ReplMethodRefreshPlan {
        let mut effective = self.clone();
        let mut refresh_by_source = Vec::new();
        for (source_ordinal, mutation) in input
            .functions
            .iter()
            .filter(|function| !crate::compile::ir_inline::is_markerless_lowered_function(function))
            .enumerate()
        {
            let mutation_identity =
                ReplMethodIdentity::from_function(&mutation.name, mutation.as_ref());
            if !effective.contains_generic(&mutation.name) {
                effective.apply(std::iter::once(mutation));
                continue;
            }
            let mut affected_names = BTreeSet::from([mutation.name.clone()]);
            let mut refresh = BTreeSet::new();
            loop {
                let mut changed = false;
                for (identity, dependencies) in &effective.dependencies {
                    if identity == &mutation_identity
                        || refresh.contains(identity)
                        || dependencies.is_disjoint(&affected_names)
                    {
                        continue;
                    }
                    refresh.insert(identity.clone());
                    affected_names.insert(identity.function().to_string());
                    changed = true;
                }
                if !changed {
                    break;
                }
            }
            for identity in refresh {
                let Some(source) = effective.methods.get(&identity) else {
                    continue;
                };
                refresh_by_source.push((source_ordinal, identity, source.clone()));
            }
            effective.apply(std::iter::once(mutation));
        }
        refresh_by_source.sort_by(|left, right| (left.0, &left.1).cmp(&(right.0, &right.1)));

        let mut plan = ReplMethodRefreshPlan::default();
        for (refresh_ordinal, (source_ordinal, _identity, source)) in
            refresh_by_source.into_iter().enumerate()
        {
            plan.methods.push(Arc::new(source));
            plan.refresh_ordinals_by_source
                .entry(source_ordinal)
                .or_default()
                .push(refresh_ordinal);
        }
        plan
    }
}

impl ReplModuleMetadata {
    /// Build the module surface from a compiled program's modules (Issue #9199
    /// LV5). `modules` is the merged program's module list (user modules +
    /// any loaded package modules); the caller decides — via the session
    /// eligibility gate — whether the delta path is taken, so carrying every
    /// module here is safe (an ineligible session never consults it).
    fn from_modules(modules: &[crate::ir::core::Module]) -> Self {
        let mut meta = ReplModuleMetadata::default();
        for module in modules {
            meta.toplevel_module_bindings.insert(module.name.clone());
            crate::compile::collect_module_publics(module, "", &mut meta.module_publics);
            crate::compile::collect::collect_module_info(
                module,
                "",
                &mut meta.module_functions,
                &mut meta.module_exports,
                &mut meta.module_constants,
            );
        }
        meta
    }

    fn is_empty(&self) -> bool {
        self.module_functions.is_empty()
    }
}

#[derive(Debug)]
pub struct ReplPersistentCompile {
    /// Reusable compile output for the accumulated program: `compiled` is the
    /// whole accumulated bytecode (base + generated + prior user); the
    /// tables/snapshot seed the next compile so the prior prefix is neither
    /// re-inferred nor re-emitted.
    bundle: super::pipeline_ctx::CoreCompileOutput,
    /// Module-surface metadata for the accumulated program's modules (Issue #9199
    /// LV5). Consulted by `repl_relocatable_delta_compile` so a delta that
    /// references a prior simple user module resolves against the live VM instead
    /// of erroring "Unknown module". Empty for a module-free session.
    module_metadata: ReplModuleMetadata,
    /// User/session imports already executed in the reused prefix. Delta IR
    /// omits those prior statements, but name resolution still needs their
    /// lexical metadata; runtime reads use the live hidden binding state.
    usings: Vec<crate::ir::core::UsingImport>,
    /// Every function NAME bound in the accumulated program (base + prior user).
    /// Carried into `CompilerCacheInput.extra_imported_functions` so a delta
    /// eval's expression can call any prior-defined function (which lives in the
    /// reused prefix, not this compile's IR, and would otherwise be rejected as
    /// "not imported"). Successful append-only generic-function deltas extend
    /// this set together with the persistent compiler snapshot; redefinitions
    /// still route to the full recompile path.
    function_names: HashSet<String>,
    /// Every TYPE NAME bound in the accumulated program's reused prefix — the
    /// names of `compiled.struct_defs` (base + prior concrete structs),
    /// `compiled.abstract_types`, and `compiled.primitive_types` (Issue #9199
    /// LV4). A struct definition whose name is present here is NOT a brand-new
    /// type — it is a base/prior struct REDEFINITION (which changes the struct's
    /// `type_id`/layout and so must recompile every `NewStruct` reference to it),
    /// so it routes to the full recompile path. Successful append-only concrete
    /// struct deltas extend this set together with the persistent snapshot.
    type_names: HashSet<String>,
    /// Prior source types that suppress the raw field-count constructor by
    /// declaring one or more inner constructors. Delta IR omits those prior
    /// structs, so this metadata must accompany the reused compiled prefix.
    inner_constructor_type_names: HashSet<String>,
    /// Latest Main-owned ordinary methods plus their conservative direct-call
    /// dependency snapshot. Method mutations compile only the transitive caller
    /// slice instead of rebuilding the whole session (Issue #9784).
    method_sources: ReplMethodSourceSnapshot,
    /// Rollback material for the most recent definition-bearing live delta.
    /// The successful path keeps the fully advanced snapshot. A catchable
    /// error projects it onto the exact activation prefix reached by the VM.
    definition_transaction: Option<ReplDefinitionTransaction>,
}

impl ReplPersistentCompile {
    /// Number of functions in the reused compile prefix (Issue #9199 LV3). The
    /// live VM built from this bundle holds exactly this many functions until a
    /// live-append grows it; the compiled-definition append is only sound while
    /// the live count still equals this (so delta indices are live-aligned).
    pub fn prefix_function_count(&self) -> usize {
        self.bundle.compiled.functions.len()
    }

    /// Number of concrete structs in the reused compile prefix (Issue #9199 LV4).
    /// A concrete struct's `type_id` IS its index in `struct_defs`, baked into
    /// every `NewStruct(type_id, ..)`; the live VM built from this bundle holds
    /// exactly this many struct defs until a type live-append grows it. A
    /// compiled-struct append is only sound while the live count still equals this
    /// (so each new struct installs at the aligned `type_id` the delta baked).
    pub fn prefix_struct_def_count(&self) -> usize {
        self.bundle.compiled.struct_defs.len()
    }

    pub fn prefix_abstract_type_count(&self) -> usize {
        self.bundle.compiled.abstract_types.len()
    }

    pub fn prefix_primitive_type_count(&self) -> usize {
        self.bundle.compiled.primitive_types.len()
    }

    pub fn prefix_enum_def_count(&self) -> usize {
        self.bundle.compiled.enum_defs.len()
    }

    /// Whether `name` is bound by the reused prefix (a Base function or a
    /// function from the last FULL compile) — Issue #9199 LV3. A name present
    /// here is NOT a brand-new generic, so a definition of it (base extension /
    /// same-name method) must take the full recompile path.
    pub fn defines_function(&self, name: &str) -> bool {
        self.function_names.contains(name)
    }

    /// Whether the aligned compiler/VM prefix contains any body for `name`,
    /// including an unreached dormant body that is intentionally absent from
    /// [`Self::defines_function`]. Defining such a name again must take the full
    /// recompile path so prior forward-reference call sites are rebuilt against
    /// the newly reached method instead of retaining the dormant index (#9784).
    pub fn contains_function_body(&self, name: &str) -> bool {
        self.bundle
            .compiled
            .functions
            .iter()
            .any(|function| function.name == name)
    }

    /// Whether an existing body belongs to a Main-owned ordinary generic whose
    /// methods can participate in the #9784 live mutation path. Base/preload and
    /// generated-only bodies deliberately remain on the full-refresh path.
    pub fn owns_source_generic(&self, name: &str) -> bool {
        self.method_sources.contains_generic(name)
    }

    /// Whether `name` is a TYPE already bound by the reused prefix (a Base or
    /// prior-user struct / abstract / primitive type) — Issue #9199 LV4. A name
    /// present here is NOT a brand-new type, so its definition is a redefinition
    /// and must take the full recompile path (a struct redefinition changes the
    /// baked `type_id` of every `NewStruct` reference).
    pub fn defines_type(&self, name: &str) -> bool {
        self.type_names.contains(name)
    }

    /// Adopt the exact runtime-conditional nominal definitions observed by the
    /// live VM. The compiler candidate contains only inert templates; registry
    /// rows are appended here after execution proves which sites committed.
    pub fn adopt_runtime_nominal_activations(
        mut self,
        activations: &[RuntimeNominalActivation],
    ) -> Option<Self> {
        let constructor_indices_by_site = self
            .bundle
            .compiled
            .code
            .iter()
            .filter_map(|instruction| match instruction {
                crate::bytecode::Instr::DefineRuntimeNominal(operands) => Some((
                    operands.site_id,
                    operands.constructor_function_indices.clone(),
                )),
                _ => None,
            })
            .collect::<HashMap<_, _>>();
        let mut sites = HashSet::new();
        for activation in activations {
            if !sites.insert(activation.site_id) {
                return None;
            }
            if activation.coalesced_root {
                continue;
            }
            match &activation.definition {
                RuntimeNominalDefInfo::Struct(definition) => {
                    if !definition.source.inner_constructors.is_empty()
                        && self.bundle.compiled.struct_defs.get(activation.registry_id)
                            == Some(&definition.layout)
                    {
                        self.type_names.insert(definition.layout.name.clone());
                        self.inner_constructor_type_names
                            .insert(definition.layout.name.clone());
                        let context = self.bundle.compiled.compile_context.as_mut()?;
                        context.struct_table.insert(
                            definition.layout.name.clone(),
                            super::StructInfo {
                                type_id: activation.registry_id,
                                is_mutable: definition.layout.is_mutable,
                                fields: definition.layout.fields.clone(),
                                has_inner_constructor: true,
                            },
                        );
                        for &index in constructor_indices_by_site.get(&activation.site_id)? {
                            let function = self.bundle.compiled.functions.get_mut(index)?;
                            std::rc::Rc::make_mut(function).min_world = 1;
                        }
                        continue;
                    }
                    if activation.registry_id != self.bundle.compiled.struct_defs.len()
                        || self.type_names.contains(&definition.layout.name)
                        || !definition.source.type_params.is_empty()
                        || !definition.source.inner_constructors.is_empty()
                    {
                        return None;
                    }
                    let context = self.bundle.compiled.compile_context.as_mut()?;
                    if context.struct_defs.len() != activation.registry_id {
                        return None;
                    }
                    context.struct_table.insert(
                        definition.layout.name.clone(),
                        super::StructInfo {
                            type_id: activation.registry_id,
                            is_mutable: definition.layout.is_mutable,
                            fields: definition.layout.fields.clone(),
                            has_inner_constructor: false,
                        },
                    );
                    context.struct_defs.push(definition.layout.clone());
                    self.bundle
                        .compiled
                        .struct_defs
                        .push(definition.layout.clone());
                    self.type_names.insert(definition.layout.name.clone());
                }
                RuntimeNominalDefInfo::AbstractType(definition) => {
                    if activation.registry_id != self.bundle.compiled.abstract_types.len()
                        || !self.type_names.insert(definition.name.clone())
                    {
                        return None;
                    }
                    self.bundle.compiled.abstract_types.push(definition.clone());
                }
                RuntimeNominalDefInfo::PrimitiveType(definition) => {
                    if activation.registry_id != self.bundle.compiled.primitive_types.len()
                        || !self.type_names.insert(definition.name.clone())
                    {
                        return None;
                    }
                    self.bundle
                        .compiled
                        .primitive_types
                        .push(definition.clone());
                    let context = self.bundle.compiled.compile_context.as_mut()?;
                    context.primitive_types = self.bundle.compiled.primitive_types.clone();
                }
                RuntimeNominalDefInfo::Enum(definition) => {
                    if activation.registry_id != self.bundle.compiled.enum_defs.len()
                        || !self.type_names.insert(definition.name.clone())
                    {
                        return None;
                    }
                    self.bundle.compiled.enum_defs.push(definition.clone());
                }
            }
        }
        Some(self)
    }

    /// Project an append-only compiler snapshot onto the exact source-ordered
    /// function-activation prefix reached before a catchable REPL error.
    ///
    /// Dormant function bodies stay in `compiled.functions` so their indices
    /// remain aligned with the already-grown live VM, but their method-table
    /// rows and binding names are removed and their `min_world` remains hidden.
    /// Reached bodies become ordinary visible prefix methods. Closure metadata
    /// for every touched generic is discarded (the appendability gate rejects
    /// closure-bearing definitions). Inference results remain available: their
    /// method-instance keys are signature-specific, while the filtered method
    /// tables remain the authority for which signatures can resolve. Keeping the
    /// reached entries also preserves relocatable direct-call compilation for the
    /// next live expression (Issues #9784 and #11477).
    pub fn retain_reached_function_prefix(
        mut self,
        first_appended_index: usize,
        appended_names: &[String],
        source_function_indices: &[usize],
        reached_count: usize,
        definition_activations: &[ReplDefinitionActivation],
        runtime_constructor_indices: &[usize],
    ) -> Option<Self> {
        if reached_count > source_function_indices.len() {
            return None;
        }
        let appended_end = first_appended_index.checked_add(appended_names.len())?;
        if appended_end != self.bundle.compiled.functions.len() {
            return None;
        }
        if self.bundle.compiled.functions[first_appended_index..appended_end]
            .iter()
            .zip(appended_names)
            .any(|(info, name)| info.name != *name)
        {
            return None;
        }
        let mut reached_members = HashSet::new();
        let mut all_activation_members = HashSet::new();
        let mut function_ordinal = 0usize;
        for activation in definition_activations {
            let (primary, refresh): (Option<usize>, &[usize]) = match activation {
                ReplDefinitionActivation::Function(index) => (Some(*index), &[]),
                ReplDefinitionActivation::FunctionGroup { primary, refresh } => {
                    (Some(*primary), refresh)
                }
                ReplDefinitionActivation::Struct(_)
                | ReplDefinitionActivation::AbstractType(_)
                | ReplDefinitionActivation::PrimitiveType(_)
                | ReplDefinitionActivation::Enum(_)
                | ReplDefinitionActivation::RuntimeNominal(_) => (None, &[]),
            };
            let Some(primary) = primary else {
                continue;
            };
            if source_function_indices.get(function_ordinal).copied() != Some(primary)
                || !(first_appended_index..appended_end).contains(&primary)
                || !all_activation_members.insert(primary)
                || refresh.iter().any(|index| {
                    !(first_appended_index..appended_end).contains(index)
                        || !all_activation_members.insert(*index)
                })
            {
                return None;
            }
            if function_ordinal < reached_count {
                reached_members.insert(primary);
                reached_members.extend(refresh.iter().copied());
            }
            function_ordinal += 1;
        }
        if function_ordinal != source_function_indices.len() {
            return None;
        }
        for &index in runtime_constructor_indices {
            if index >= self.bundle.compiled.functions.len() || !reached_members.insert(index) {
                return None;
            }
        }

        if reached_count == source_function_indices.len() {
            return Some(self);
        }
        let Some(transaction) = self.definition_transaction.as_ref() else {
            // A fresh full compile has no live-append rollback transaction, but
            // its method tables and source snapshot still carry stable global
            // indices. Project the unreached method suffix directly so an
            // already committed runtime nominal can survive the same uncaught
            // error (Issue #11683).
            let unreached_members = all_activation_members
                .difference(&reached_members)
                .copied()
                .collect::<HashSet<_>>();
            let unreached_identities = self
                .bundle
                .method_tables
                .iter()
                .flat_map(|(name, table)| {
                    table
                        .methods
                        .iter()
                        .filter(|method| unreached_members.contains(&method.global_index))
                        .map(|method| ReplMethodIdentity::from_method_sig(name, method))
                        .collect::<Vec<_>>()
                })
                .collect::<HashSet<_>>();

            for (index, info) in self.bundle.compiled.functions.iter_mut().enumerate() {
                if unreached_members.contains(&index) {
                    std::rc::Rc::make_mut(info).min_world = u64::MAX;
                }
            }
            for table in self.bundle.method_tables.values_mut() {
                let retained = table
                    .methods
                    .iter()
                    .filter(|method| !unreached_members.contains(&method.global_index))
                    .cloned()
                    .collect::<Vec<_>>();
                let mut projected = table.clone_with_methods_for_compile(Vec::new());
                for method in retained {
                    projected.add_method(method);
                }
                *table = projected;
            }
            self.method_sources
                .methods
                .retain(|identity, _| !unreached_identities.contains(identity));
            self.method_sources
                .dependencies
                .retain(|identity, _| !unreached_identities.contains(identity));
            self.bundle
                .compiled
                .specializable_functions
                .retain(|function| !unreached_members.contains(&function.fallback_index));
            for index in &source_function_indices[reached_count..] {
                let info = self.bundle.compiled.functions.get(*index)?;
                let still_bound = self
                    .bundle
                    .method_tables
                    .get(&info.name)
                    .is_some_and(|table| !table.methods.is_empty());
                if !still_bound {
                    self.function_names.remove(&info.name);
                }
            }
            self.bundle.inference_results.clear();
            return Some(self);
        };
        if transaction.source_functions.len() != source_function_indices.len()
            || transaction
                .source_functions
                .iter()
                .zip(source_function_indices)
                .any(|((index, _), expected)| index != expected)
            || transaction.initial_specializable_functions.len()
                != self.bundle.compiled.specializable_functions.len()
        {
            return None;
        }
        let transaction = self.definition_transaction.take()?;

        for (index, info) in self.bundle.compiled.functions[first_appended_index..appended_end]
            .iter_mut()
            .enumerate()
            .map(|(offset, info)| (first_appended_index + offset, info))
        {
            std::rc::Rc::make_mut(info).min_world =
                if reached_members.contains(&index) || !all_activation_members.contains(&index) {
                    1
                } else {
                    u64::MAX
                };
        }

        let current_method_tables = std::mem::take(&mut self.bundle.method_tables);
        let touched_current_method_tables = transaction
            .prior_method_tables
            .keys()
            .filter_map(|name| {
                current_method_tables
                    .get(name)
                    .cloned()
                    .map(|table| (name.clone(), table))
            })
            .collect::<HashMap<_, _>>();
        let mut recovered_method_tables = current_method_tables;
        for (name, prior) in &transaction.prior_method_tables {
            match prior {
                Some(table) => {
                    recovered_method_tables.insert(name.clone(), table.clone());
                }
                None => {
                    recovered_method_tables.remove(name);
                }
            }
        }
        // Marker-less helper rows are active from installation and survive the
        // projection. Add them only when they do not collide with an existing
        // Julia-visible signature; helper provenance never overrides a source
        // method merely because its internal spelling happens to match.
        for (table_name, table) in &touched_current_method_tables {
            for method in table.methods.iter().filter(|method| {
                transaction
                    .markerless_function_indices
                    .contains(&method.global_index)
            }) {
                let recovered = recovered_method_tables
                    .entry(table_name.clone())
                    .or_insert_with(|| table.clone_with_methods_for_compile(Vec::new()));
                if recovered
                    .ordinary_method_with_same_signature(method)
                    .is_none()
                {
                    recovered.add_method(method.clone());
                }
            }
        }
        // Replay the exact reached source rows in activation order. The final
        // table may contain only a later same-signature row, so filtering it is
        // insufficient; `method_rows_by_index` preserves every replaced row.
        let mut replayed_source_count = 0usize;
        for activation in definition_activations {
            let indices: Vec<usize> = match activation {
                ReplDefinitionActivation::Function(index) => vec![*index],
                ReplDefinitionActivation::FunctionGroup { primary, refresh } => {
                    let mut indices = Vec::with_capacity(1 + refresh.len());
                    indices.push(*primary);
                    indices.extend(refresh.iter().copied());
                    indices
                }
                ReplDefinitionActivation::Struct(_)
                | ReplDefinitionActivation::AbstractType(_)
                | ReplDefinitionActivation::PrimitiveType(_)
                | ReplDefinitionActivation::Enum(_)
                | ReplDefinitionActivation::RuntimeNominal(_) => continue,
            };
            if replayed_source_count >= reached_count {
                break;
            }
            for index in indices {
                let (table_name, row) = transaction.method_rows_by_index.get(&index)?;
                let template = touched_current_method_tables.get(table_name)?;
                recovered_method_tables
                    .entry(table_name.clone())
                    .or_insert_with(|| template.clone_with_methods_for_compile(Vec::new()))
                    .add_method(row.clone());
            }
            replayed_source_count += 1;
        }
        if replayed_source_count != reached_count {
            return None;
        }
        self.bundle.method_tables = recovered_method_tables;

        self.function_names = transaction.prior_function_names;
        self.function_names.extend(
            transaction.source_functions[..reached_count]
                .iter()
                .map(|(_, function)| function.name.clone()),
        );
        self.method_sources = transaction.prior_method_sources;
        self.method_sources.apply(
            transaction.source_functions[..reached_count]
                .iter()
                .map(|(_, function)| function),
        );

        self.bundle.compiled.specializable_functions = transaction.initial_specializable_functions;
        for (index, update) in transaction.specializable_updates {
            if reached_members.contains(&update.fallback_index) {
                *self
                    .bundle
                    .compiled
                    .specializable_functions
                    .get_mut(index)? = update;
            }
        }

        // The full delta was inferred with the dormant suffix installed. An
        // unreached later definition may replace a reached method under the
        // same stable signature while leaving a return cache keyed only by that
        // signature. Once method tables are projected back to the reached row,
        // no cached result can prove which body produced it; clear the cache as
        // the nominal partial-recovery paths below already do (Issue #9784).
        self.bundle.inference_results.clear();

        Some(self)
    }

    /// Project the concrete-type tail onto the exact declaration prefix whose
    /// `DefineEvalStruct` markers ran before a catchable REPL error.
    pub fn retain_reached_struct_prefix(
        mut self,
        first_appended_type_id: usize,
        appended_names: &[String],
        reached_count: usize,
    ) -> Option<Self> {
        if reached_count > appended_names.len() {
            return None;
        }
        let appended_end = first_appended_type_id.checked_add(appended_names.len())?;
        if appended_end != self.bundle.compiled.struct_defs.len()
            || self.bundle.compiled.struct_defs[first_appended_type_id..appended_end]
                .iter()
                .zip(appended_names)
                .any(|(definition, name)| definition.name != *name)
        {
            return None;
        }
        if reached_count == appended_names.len() {
            return Some(self);
        }

        let cutoff = first_appended_type_id.checked_add(reached_count)?;
        let unreached_names: HashSet<&str> = appended_names[reached_count..]
            .iter()
            .map(String::as_str)
            .collect();
        self.bundle.compiled.struct_defs.truncate(cutoff);
        if let Some(context) = self.bundle.compiled.compile_context.as_mut() {
            context.struct_defs.truncate(cutoff);
            context.struct_table.retain_type_ids_below(cutoff);
        }
        self.bundle
            .method_tables
            .retain(|name, _| !unreached_names.contains(name.as_str()));
        self.bundle
            .closure_captures
            .retain(|name, _| !unreached_names.contains(name.as_str()));
        // Cached inference may retain JuliaType/ValueType identities from the
        // discarded tail. Rebuilding it lazily is safer than trying to prove
        // every cache key and return projection independent of those IDs.
        self.bundle.inference_results.clear();
        self.type_names = collect_type_names(&self.bundle.compiled);
        Some(self)
    }

    /// Project the abstract, primitive, and enum registry tails onto the exact
    /// declaration prefix whose source markers ran before a catchable REPL
    /// error. Each registry has its own aligned index space, but all three are
    /// validated and truncated as one transaction so the next live append can
    /// safely reuse every discarded suffix slot (Issue #9784).
    #[allow(clippy::too_many_arguments)]
    pub fn retain_reached_nominal_prefixes(
        mut self,
        first_appended_abstract_type_id: usize,
        appended_abstract_type_names: &[String],
        reached_abstract_type_count: usize,
        first_appended_primitive_type_id: usize,
        appended_primitive_type_names: &[String],
        reached_primitive_type_count: usize,
        first_appended_enum_id: usize,
        appended_enum_names: &[String],
        reached_enum_count: usize,
    ) -> Option<Self> {
        if reached_abstract_type_count > appended_abstract_type_names.len()
            || reached_primitive_type_count > appended_primitive_type_names.len()
            || reached_enum_count > appended_enum_names.len()
        {
            return None;
        }

        let abstract_type_end =
            first_appended_abstract_type_id.checked_add(appended_abstract_type_names.len())?;
        let primitive_type_end =
            first_appended_primitive_type_id.checked_add(appended_primitive_type_names.len())?;
        let enum_end = first_appended_enum_id.checked_add(appended_enum_names.len())?;
        if abstract_type_end != self.bundle.compiled.abstract_types.len()
            || primitive_type_end != self.bundle.compiled.primitive_types.len()
            || enum_end != self.bundle.compiled.enum_defs.len()
            || self.bundle.compiled.abstract_types
                [first_appended_abstract_type_id..abstract_type_end]
                .iter()
                .zip(appended_abstract_type_names)
                .any(|(definition, name)| definition.name != *name)
            || self.bundle.compiled.primitive_types
                [first_appended_primitive_type_id..primitive_type_end]
                .iter()
                .zip(appended_primitive_type_names)
                .any(|(definition, name)| definition.name != *name)
            || self.bundle.compiled.enum_defs[first_appended_enum_id..enum_end]
                .iter()
                .zip(appended_enum_names)
                .any(|(definition, name)| definition.name != *name)
        {
            return None;
        }
        if reached_abstract_type_count == appended_abstract_type_names.len()
            && reached_primitive_type_count == appended_primitive_type_names.len()
            && reached_enum_count == appended_enum_names.len()
        {
            return Some(self);
        }

        let abstract_type_cutoff =
            first_appended_abstract_type_id.checked_add(reached_abstract_type_count)?;
        let primitive_type_cutoff =
            first_appended_primitive_type_id.checked_add(reached_primitive_type_count)?;
        let enum_cutoff = first_appended_enum_id.checked_add(reached_enum_count)?;
        let unreached_names: HashSet<&str> = appended_abstract_type_names
            [reached_abstract_type_count..]
            .iter()
            .chain(appended_primitive_type_names[reached_primitive_type_count..].iter())
            .chain(appended_enum_names[reached_enum_count..].iter())
            .map(String::as_str)
            .collect();

        self.bundle
            .compiled
            .abstract_types
            .truncate(abstract_type_cutoff);
        self.bundle
            .compiled
            .primitive_types
            .truncate(primitive_type_cutoff);
        self.bundle.compiled.enum_defs.truncate(enum_cutoff);
        if let Some(context) = self.bundle.compiled.compile_context.as_mut() {
            context.primitive_types = self.bundle.compiled.primitive_types.clone();
        }
        self.bundle
            .method_tables
            .retain(|name, _| !unreached_names.contains(name.as_str()));
        self.bundle
            .closure_captures
            .retain(|name, _| !unreached_names.contains(name.as_str()));
        self.bundle.inference_results.clear();
        self.type_names = collect_type_names(&self.bundle.compiled);
        Some(self)
    }
}

/// Collect every prefix-bound TYPE name from a compiled program (Issue #9199
/// LV4): concrete struct names, abstract-type names, and primitive-type names.
/// Used to populate `ReplPersistentCompile::type_names` so a redefinition of an
/// existing type routes to the full recompile path.
fn collect_type_names(compiled: &CompiledProgram) -> HashSet<String> {
    let mut names: HashSet<String> = HashSet::new();
    names.extend(compiled.struct_defs.iter().map(|d| d.name.clone()));
    names.extend(compiled.abstract_types.iter().map(|a| a.name.clone()));
    names.extend(compiled.primitive_types.iter().map(|p| p.name.clone()));
    names.extend(
        compiled
            .enum_defs
            .iter()
            .map(|definition| definition.name.clone()),
    );
    names
}

fn collect_inner_constructor_type_names(program: &Program) -> HashSet<String> {
    super::collect::collect_inner_constructor_flags(program.structs.iter(), program.modules.iter())
        .into_iter()
        .filter_map(|(name, has_inner)| has_inner.then_some(name))
        .collect()
}

pub(crate) fn repl_current_runtime_nominal_names(program: &Program) -> HashSet<String> {
    let mut names = super::pipeline_ctx::collect_runtime_nominal_names_in_block(&program.main);
    for module in &program.modules {
        super::collect::collect_module_runtime_nominal_names(module, "", &mut names);
    }
    names
}

pub(crate) fn repl_current_type_names(program: &Program) -> HashSet<String> {
    fn collect_root_enum_names(block: &crate::ir::core::Block, names: &mut HashSet<String>) {
        for statement in &block.stmts {
            match statement {
                crate::ir::core::Stmt::EnumDef { enum_def, .. } => {
                    names.insert(enum_def.name.clone());
                }
                crate::ir::core::Stmt::Block(inner) => collect_root_enum_names(inner, names),
                _ => {}
            }
        }
    }

    fn collect_module(module: &crate::ir::core::Module, prefix: &str, names: &mut HashSet<String>) {
        let module_path = if prefix.is_empty() {
            module.name.clone()
        } else {
            format!("{prefix}.{}", module.name)
        };
        names.extend(
            module
                .structs
                .iter()
                .map(|definition| format!("{module_path}.{}", definition.name)),
        );
        names.extend(
            module
                .abstract_types
                .iter()
                .map(|definition| format!("{module_path}.{}", definition.name)),
        );
        names.extend(
            module
                .primitive_types
                .iter()
                .map(|definition| format!("{module_path}.{}", definition.name)),
        );
        super::collect::collect_module_runtime_nominal_names(module, prefix, names);
        for nested in &module.submodules {
            collect_module(nested, &module_path, names);
        }
    }

    let mut names = repl_current_runtime_nominal_names(program);
    names.extend(
        program
            .structs
            .iter()
            .map(|definition| definition.name.clone()),
    );
    names.extend(
        program
            .abstract_types
            .iter()
            .map(|definition| definition.name.clone()),
    );
    names.extend(
        program
            .primitive_types
            .iter()
            .map(|definition| definition.name.clone()),
    );
    collect_root_enum_names(&program.main, &mut names);
    for module in &program.modules {
        collect_module(module, "", &mut names);
    }
    names
}

/// Compile `merged` through the Base cache and return the FULL reusable bundle
/// (not just the `CompiledProgram`), so a `Persistent` session can carry it
/// forward for input-delta compiles (Issue #9199 S5). Mirrors the Base-cache
/// branch of [`compile_with_cache_with_globals`] but skips `PROGRAM_CACHE` (it
/// stores only `CompiledProgram`, and this path is the slow, cache-refreshing
/// one anyway).
///
/// `skip_decision_program` is the pre-merge program the base-cache-bypass
/// decision keys on — EXACTLY the program `compile_with_cache_with_globals`
/// feeds `should_skip_base_cache_for_program` (for the REPL that is the
/// un-merged input, `base_function_count == 0`, so the decision is `false` and
/// this path never diverges from Legacy by activating a bypass Legacy skipped).
fn compile_core_bundle_with_base_cache(
    skip_decision_program: &Program,
    merged: &Program,
    global_types: &HashMap<String, ValueType>,
    global_struct_names: &HashMap<String, String>,
    repl_current_function_count: Option<usize>,
    repl_current_struct_count: Option<usize>,
    current_input_type_names: Option<&HashSet<String>>,
    current_input_runtime_nominal_names: Option<&HashSet<String>>,
) -> CResult<super::pipeline_ctx::CoreCompileOutput> {
    if is_cache_disabled() {
        return crate::compile::compile_core_program_internal(
            merged,
            global_types,
            global_struct_names,
            crate::compile::CompilerCacheInput {
                current_input_type_names,
                current_input_runtime_nominal_names,
                repl_current_function_count,
                repl_current_struct_count,
                ..Default::default()
            },
        );
    }

    let prelude_function_count = crate::get_prelude_program()
        .map(|p| p.functions.len())
        .unwrap_or(skip_decision_program.base_function_count);
    if should_skip_base_cache_for_program(skip_decision_program, prelude_function_count) {
        return crate::compile::compile_core_program_internal(
            merged,
            global_types,
            global_struct_names,
            crate::compile::CompilerCacheInput {
                current_input_type_names,
                current_input_runtime_nominal_names,
                repl_current_function_count,
                repl_current_struct_count,
                ..Default::default()
            },
        );
    }

    let base_cache = get_or_init_base_cache()?;
    let preload_cache_handle = super::preload_cache::get_or_init_preload_cache();
    crate::compile::compile_core_program_internal(
        merged,
        global_types,
        global_struct_names,
        crate::compile::CompilerCacheInput {
            current_input_type_names,
            current_input_runtime_nominal_names,
            precompiled_base: Some(&base_cache.compiled),
            method_tables: Some(&base_cache.method_tables),
            closure_captures: Some(&base_cache.closure_captures),
            inference_results: Some(&base_cache.inference_results),
            preload_cache: preload_cache_handle.as_ref().map(|c| &c.modules),
            preload_closure_layout: preload_cache_handle
                .as_ref()
                .map(|c| c.closure_layout.as_slice()),
            extra_imported_functions: None,
            global_slot_seed: None,
            extra_module_metadata: None,
            extra_inner_constructor_type_names: None,
            repl_current_function_count,
            repl_current_struct_count,
            repl_append_only_new_generics: false,
        },
    )
}

/// Full accumulate-and-recompile for a `Persistent` eval that is NOT delta-safe
/// (first eval, redefinition, structs/modules/macros, base extension, …). Merges
/// the Base prelude in (if not already merged), compiles the whole program, and
/// returns the compiled program plus a fresh [`ReplPersistentCompile`] the
/// session stores so subsequent appends can go through [`repl_delta_compile`]
/// (Issue #9199 S5).
pub(crate) fn repl_full_compile(
    program: &Program,
    global_types: &HashMap<String, ValueType>,
    global_struct_names: &HashMap<String, String>,
    current_input_function_count: usize,
    current_input_struct_count: usize,
    current_input_type_names: &HashSet<String>,
    current_input_runtime_nominal_names: &HashSet<String>,
) -> CResult<(CompiledProgram, ReplPersistentCompile)> {
    let merged = if program.base_function_count > 0 {
        program.clone()
    } else {
        crate::compile::base_merge::merge_with_precompiled_base(program).program
    };
    let bundle = compile_core_bundle_with_base_cache(
        program,
        &merged,
        global_types,
        global_struct_names,
        Some(current_input_function_count),
        Some(current_input_struct_count),
        Some(current_input_type_names),
        Some(current_input_runtime_nominal_names),
    )?;
    let compiled = bundle.compiled.clone();
    let main_inline_named_functions =
        super::repl_support::collect_main_inline_named_functions(program)
            .into_iter()
            .map(Arc::new)
            .collect::<Vec<_>>();
    let mut function_names = merged
        .functions
        .iter()
        .enumerate()
        .filter(|(index, function)| {
            *index < merged.base_function_count
                || !crate::compile::ir_inline::is_markerless_lowered_function(function)
        })
        .map(|(_, function)| function.name.clone())
        .collect::<HashSet<_>>();
    function_names.extend(
        main_inline_named_functions
            .iter()
            .map(|function| function.name.clone()),
    );
    let type_names = collect_type_names(&compiled);
    let inner_constructor_type_names = collect_inner_constructor_type_names(&merged);
    // Module surface for a later delta that references a prior module (Issue
    // #9199 LV5). `merged.modules` holds every module realized this eval.
    let module_metadata = ReplModuleMetadata::from_modules(&merged.modules);
    let method_sources = ReplMethodSourceSnapshot::from_functions(
        program
            .functions
            .iter()
            .chain(main_inline_named_functions.iter()),
    );
    Ok((
        compiled,
        ReplPersistentCompile {
            bundle,
            function_names,
            type_names,
            inner_constructor_type_names,
            module_metadata,
            usings: program.usings.clone(),
            method_sources,
            definition_transaction: None,
        },
    ))
}

/// Input-delta compile for a `Persistent` eval (Issue #9199 S5). Compiles ONLY
/// the new `input` (its brand-new functions + `main`) against the accumulated
/// program by reusing `prev.bundle` as the precompiled prefix — standing in for
/// the Base cache in the ordinary reuse path. Prior user functions are reused
/// verbatim from `prev.bundle.compiled`; the pipeline compiles just the new
/// input and appends it. This is what makes per-eval compile cost independent of
/// session length.
///
/// The IR handed to the pipeline is exactly `merge_with_precompiled_base(input)`
/// — `[base source | NEW user]` — the SAME shape the full path uses. Its base
/// prefix aligns index-for-index with `prev.bundle.compiled` (both begin with
/// the identical Base source functions), and the accumulated generated + prior
/// user functions are carried through the reused prefix, NOT the IR. This
/// sidesteps the `[base | generated | user]` (compiled) vs `[base | user]` (IR)
/// index-space mismatch that trying to reconstruct the prior IR would hit —
/// compilation appends inner constructors / lifted lambdas after the source
/// functions, so the compiled function list is never the shape of the IR.
///
/// Preconditions the caller (`REPLSession`) guarantees: `input` defines no
/// modules/macros/structs/abstract/primitive types, no `using`, no opaque
/// `eval`, and every input function name is brand new
/// (`!prev.defines_function(name)`), so appending its compiled body without
/// touching the reused prefix is observationally identical to a full recompile.
pub(crate) fn repl_delta_compile(
    prev: &ReplPersistentCompile,
    input: &Program,
    global_types: &HashMap<String, ValueType>,
    global_struct_names: &HashMap<String, String>,
) -> CResult<(CompiledProgram, ReplPersistentCompile)> {
    let mut merged = if input.base_function_count > 0 {
        input.clone()
    } else {
        crate::compile::base_merge::merge_with_precompiled_base(input).program
    };
    merged.usings.extend(prev.usings.iter().cloned());

    let bundle = crate::compile::compile_core_program_internal(
        &merged,
        global_types,
        global_struct_names,
        crate::compile::CompilerCacheInput {
            precompiled_base: Some(&prev.bundle.compiled),
            method_tables: Some(&prev.bundle.method_tables),
            closure_captures: Some(&prev.bundle.closure_captures),
            inference_results: Some(&prev.bundle.inference_results),
            // Prior user functions live in the reused prefix, not this IR
            // (Issue #9199 S5); mark them accessible for name resolution.
            extra_imported_functions: Some(&prev.function_names),
            extra_inner_constructor_type_names: Some(&prev.inner_constructor_type_names),
            repl_current_function_count: None,
            repl_append_only_new_generics: true,
            ..Default::default()
        },
    )?;
    let compiled = bundle.compiled.clone();

    let mut function_names = prev.function_names.clone();
    function_names.extend(
        input
            .functions
            .iter()
            .filter(|function| !crate::compile::ir_inline::is_markerless_lowered_function(function))
            .map(|function| function.name.clone()),
    );
    let type_names = collect_type_names(&compiled);
    let mut method_sources = prev.method_sources.clone();
    method_sources.apply(&input.functions);

    Ok((
        compiled,
        ReplPersistentCompile {
            bundle,
            function_names,
            type_names,
            inner_constructor_type_names: prev.inner_constructor_type_names.clone(),
            // A delta defines no modules, so the module surface is unchanged from
            // the reused prefix (Issue #9199 LV5).
            module_metadata: prev.module_metadata.clone(),
            usings: prev.usings.clone(),
            method_sources,
            definition_transaction: None,
        },
    ))
}

/// Whether the extracted user-main bytecode references ONLY functions that
/// installed in the VM for this delta — i.e. every function index it touches is
/// `< threshold`, and every name-only closure target is present in
/// `installed_function_names` (Issue #9199 LV2 / #11569). `threshold` is the
/// reused prefix plus the structurally verified new-function tail. A `false`
/// result means the bytecode reaches a body outside that installed region, so
/// the caller must fall back to the fresh path.
///
/// Conservative by construction: the direct-index, dynamic-candidate, AND
/// fallback/target-index calls are decoded exactly — EVERY function-table index an
/// arm carries (fallback function index as well as the candidate list, Issue #9199
/// review r3536211262 / r3536211255) is threshold-checked, not just one list;
/// runtime-specialization indices are checked against their separately restored
/// prefix table; by-value call/invoke consumers carry no target index and rely
/// on the independently checked callable producers in the same scan. Static
/// generator callables are decoded like direct calls; runtime generator forms
/// consume independently checked callable values. Remaining undecoded forms
/// (by-name calls, global refs, function definitions)
/// return `false`.
/// Non-function instructions (loads/stores/arithmetic/jumps/collections/builtins)
/// reference no function and are always safe.
fn user_main_calls_only_existing_functions(
    code: &[crate::bytecode::Instr],
    threshold: usize,
    safe_specializable_indices: &HashSet<usize>,
    installed_function_names: &HashSet<&str>,
) -> bool {
    use crate::bytecode::{DynamicCallCandidate, Instr};
    code.iter().all(|instr| match instr {
        // Direct function-index calls — safe iff the callee already lives in the VM.
        Instr::Call(f, _)
        | Instr::CallInbounds(f, _)
        | Instr::CallWithKwargs(f, _, _)
        | Instr::CallWithKwargsSplat(f, _, _, _)
        | Instr::CallWithSplat(f, _, _)
        | Instr::CallResolved(f, _) => *f < threshold,
        Instr::CallResolvedI64Slots(b) | Instr::CallInboundsI64Slots(b) => b.func_index < threshold,
        Instr::CallStaticParametric(b) => {
            b.func_index < threshold
                && b.validation_fallback
                    .as_ref()
                    .is_none_or(|fallback| fallback.func_index < threshold)
        }
        // Direct-index HOF forms that immediately CALL a statically resolved
        // function during the main (Issue #9199 review r3535721788). `ntuple(f, n)`
        // and `sprint(f, args...)` bake `f`'s function index and invoke it in place,
        // so they are safe iff that callee already lives in the VM — index-check
        // them exactly like `Call`. A FRESH lambda (`ntuple(i -> i, n)`) is lifted
        // at an index ≥ threshold (its body in the un-spliced suffix), so this
        // rejects → the caller falls back to the fresh path instead of dispatching
        // to a function the live VM never installed.
        Instr::NtupleFunc(f) => *f < threshold,
        Instr::SprintFunc(f, _) => *f < threshold,
        Instr::MakeGenerator(operands) => match &operands.callable {
            crate::bytecode::GeneratorCallableSpec::FunctionIndex(index)
            | crate::bytecode::GeneratorCallableSpec::TupleSplatFunctionIndex(index) => {
                *index < threshold
            }
            crate::bytecode::GeneratorCallableSpec::FilteredFunctionIndex {
                map_func_index,
                predicate_func_index,
            } => *map_func_index < threshold && *predicate_func_index < threshold,
        },
        // Dynamic dispatch — safe iff EVERY function-table index the VM may
        // dispatch to already lives in the VM. The Call family carries such
        // indices in MORE than the candidate list: several variants also bake a
        // FALLBACK/target function index the runtime dispatches to when no
        // candidate wins. The gate must threshold-check ALL of them, not just
        // `candidates` (Issue #9199 review r3536211262 / r3536211255 — the 3rd
        // round of findings on this gate). Missing one accepts a delta whose
        // fallback body lives only in the discarded fresh suffix, then dispatches
        // to a function index the held live VM never installed.
        //
        // `CallDynamic`'s FIRST operand is a real fallback function index the VM
        // dispatches to for generic-collect fallback and zero-arg dynamic dispatch
        // (`start_function_call(*fallback_func_index)` in exec/call_dynamic.rs).
        // `usize::MAX` is the explicit no-fallback sentinel and references no
        // function body; only a non-sentinel fallback must be installed.
        Instr::CallDynamic(operands) => {
            (operands.fallback_func_index == usize::MAX || operands.fallback_func_index < threshold)
                && operands.candidates.iter().all(|c| match c {
                    DynamicCallCandidate::Method(i) => *i < threshold,
                    DynamicCallCandidate::NativeIterator(_) => true,
                })
        }
        // `CallTypedDispatch`'s THIRD operand is the fallback function index the
        // runtime selects when no `Type{T}` candidate wins
        // (`select_typed_dispatch_candidate(*fallback_index, ..)` in
        // exec/call_dynamic_typed.rs), so it must be installed too.
        Instr::CallTypedDispatch(_, _, fallback, cs) => {
            *fallback < threshold && cs.iter().all(|i| *i < threshold)
        }
        // `CallParametricConstructorDispatch`'s candidates each carry their own
        // `func_index` (Issue #10968/#10971) — every candidate must already
        // live in the VM, same as `CallTypedDispatch`'s candidate list.
        Instr::CallParametricConstructorDispatch(b) => {
            b.candidates.iter().all(|c| c.func_index < threshold)
        }
        // `CallDynamicBinary`'s FIRST operand is a documented fallback function
        // index (`(fallback_func_index, check_position, candidate_func_indices)`).
        // Today's VM handler happens to raise a MethodError instead of dispatching
        // to it, but the operand IS a function-table reference; index-checking it
        // keeps the gate sound at the OPERAND level so a future dispatch to it can
        // never reach an uninstalled body (systemic fix, Issue #9199 review).
        Instr::CallDynamicBinary(fallback, _, cs) => {
            *fallback < threshold && cs.iter().all(|i| *i < threshold)
        }
        // The remaining dynamic forms' first operand is NOT a function-table index
        // (an `Intrinsic` for `CallDynamicBinaryBoth`, a `BuiltinId` for the
        // `*OrBuiltin*` forms, or an arg-count `usize` for
        // `CallTypedDispatchOrBuiltin[Result]`), so only the candidate list holds
        // function indices to check.
        Instr::CallDynamicBinaryBoth(_, cs)
        | Instr::CallDynamicBinaryNoFallback(cs)
        | Instr::CallDynamicOrBuiltin(_, cs)
        | Instr::CallTypedDispatchOrBuiltin(_, _, _, cs)
        | Instr::CallTypedDispatchOrBuiltinResult(_, _, _, cs) => cs.iter().all(|i| *i < threshold),
        // `iterate(collection)` on an `Any`-typed collection dispatches to one of
        // `candidates` (iterate methods) at runtime — safe iff every candidate is
        // already installed (the leading `usize` is the arg count, not an index).
        Instr::IterateDynamic(_, candidates) => candidates.iter().all(|i| *i < threshold),
        Instr::CallTypedDispatchOrBuiltinStoreDict(b)
        | Instr::CallTypedDispatchOrBuiltinStoreDictResult(b) => {
            b.candidates.iter().all(|i| *i < threshold)
        }
        Instr::PushResolvedFunction(b) => b.candidate_indices.iter().all(|i| *i < threshold),
        // Builtins / intrinsics touch no function-table entry.
        Instr::CallIntrinsic(_) | Instr::CallBuiltin(_, _) => true,
        // Source-ordered activation of a function body installed by this same
        // delta is safe exactly when its index is within the installed prefix +
        // verified new-function tail. An out-of-range activation would mutate a
        // function-table entry whose body was discarded, so reject it just like
        // a direct call (Issue #9784).
        Instr::DefineEvalFunction(index) => *index < threshold,
        Instr::ActivateUsing { .. } | Instr::ActivateModule(..) => true,
        // Type IDs are independently validated by the nominal-registry append
        // gates; the markers themselves carry no function-table reference.
        Instr::DefineEvalStruct(_)
        | Instr::DefineEvalAbstractType(_)
        | Instr::DefineEvalPrimitiveType(_)
        | Instr::DefineRuntimeNominal(_) => true,
        // `CreateClosure` names its body rather than carrying an index. Admit it
        // only when the exact name belongs to the verified installed prefix/new
        // tail. A nested/generated body outside that tail is absent from this set
        // and therefore still fails closed (Issue #11569 / #9199).
        Instr::CreateClosure { func_name, .. } => {
            installed_function_names.contains(func_name.as_str())
        }
        Instr::CreateResolvedClosure(operands) => operands
            .candidate_indices
            .iter()
            .all(|index| *index < threshold),
        // Runtime specialization indexes a separate table. The precompiled
        // prefix is safe only when the fresh compile retained the exact same
        // name/fallback pair at that index. A mere numeric prefix check can
        // accidentally admit a later source definition that reused an old
        // specialization slot and make it callable before its source marker.
        Instr::CallSpecialize(index, _) | Instr::CallSpecializeInbounds(index, _) => {
            safe_specializable_indices.contains(index)
        }
        Instr::CallSpecializeI64Slots(operands)
        | Instr::CallSpecializeInboundsI64Slots(operands)
        | Instr::CallSpecializeF64Slots(operands)
        | Instr::CallSpecializeInboundsF64Slots(operands) => {
            safe_specializable_indices.contains(&operands.spec_func_index)
        }
        // These forms carry no function-table reference: they consume a
        // runtime callable Value already on the stack. Every bytecode producer
        // that can introduce such a value (`CreateClosure`,
        // `PushResolvedFunction`, direct calls, etc.) is independently checked
        // by this same all-instruction scan, so the consumer itself is safe to
        // splice (Issue #11569).
        Instr::CallFunctionVariable(_)
        | Instr::CallFunctionVariableWithSplat(_, _)
        | Instr::CallFunctionVariableWithKwargsSplat(_)
        | Instr::InvokeFunctionVariable(_, _)
        | Instr::InvokeFunctionVariableWithKwargs(_)
        | Instr::InvokeFunctionVariableDynamicSignature(_)
        | Instr::InvokeFunctionVariableDynamicSignatureWithKwargs(_, _, _)
        | Instr::MakeGeneratorRuntime(_, _)
        | Instr::MakeGeneratorRuntimeFiltered(_) => true,
        // Function-referencing / function-lifting forms this scan does not
        // decode to a checkable installed target ⇒ conservative reject. This
        // includes by-name calls and dynamic function definition.
        Instr::CallGlobalRef(_)
        | Instr::PushFunction(_)
        | Instr::DefineFunction(_)
        // Same by-name runtime lookup as `PushFunction`/`CallGlobalRef` (Issue
        // #11320): it resolves `FunctionInfo` entries by name at the moment it
        // runs, which could observe a function whose body lives only in the
        // un-spliced fresh suffix. No checkable index of its own — conservative
        // reject, consistent with its by-name siblings.
        | Instr::RaiseUndefVarErrorIfFunctionInvisible(_) => false,
        // Everything else references no function-table entry and is safe to splice.
        //
        // FAIL-CLOSED CONTRACT (Issue #9199 review r3535721788): this catch-all is
        // sound ONLY because every `Instr` that carries a function index or
        // lifts/creates a function/closure is handled by an explicit arm above. A
        // NEW such opcode that lands here would be treated as silently safe and
        // spliced onto the live VM → dispatch to an uninstalled function. When you
        // add a function-index / closure-bearing `Instr` variant, add it to a
        // checked (`< threshold`) or reject (`=> false`) arm above AND to
        // `FUNCTION_BEARING_INSTRS` in the tests below. The frozen-variant-count
        // guard `live_append_gate_function_bearing_classification_is_fail_closed_9199`
        // trips on any new variant so the decision cannot be skipped.
        _ => true,
    })
}

/// A relocatable delta main ready to splice onto a live VM (Issue #9199 LV2 —
/// the crux). `new_main` is the isolated user-main bytecode with its intra-block
/// jumps already relocated onto the live VM's code tail and its global slots
/// already aligned to the live frame-0 by the seeded compile; `new_globals` are
/// the brand-new globals this delta binds (grow frame-0 by these). Produced only
/// when the compile is CLEANLY appendable — see `repl_relocatable_delta_compile`.
#[derive(Debug)]
pub struct AppendableDelta {
    /// The compiled bodies of the brand-new user functions this delta DEFINES
    /// (Issue #9199 LV3 — empty for an expression/global delta). Each carries its
    /// bytecode with jumps already relocated onto the live VM's code tail and a
    /// `FunctionInfo` whose `entry`/`code_start`/`code_end` are the final live
    /// positions, in install order. They are spliced BEFORE `new_main` (so the
    /// main lands after them). The caller guarantees the live VM holds exactly
    /// `prev.bundle.compiled.functions.len()` functions, so each function's
    /// delta index equals its live index and no function-index relocation is
    /// needed (a body's calls to base/prior functions stay aligned, and calls to
    /// a same-batch function land on that function's aligned live index).
    pub new_functions: Vec<AppendableFunction>,
    /// Final aligned indices of Julia-visible source methods, in activation
    /// order. Marker-less lowering helpers are absent and immediately visible.
    pub source_function_indices: Vec<usize>,
    /// The brand-new CONCRETE struct definitions this delta DEFINES (Issue #9199
    /// LV4 — empty for an expression/global/function delta), in declaration
    /// order. A concrete struct's `type_id` IS its index in `struct_defs`, so the
    /// pipeline (seeded from the reused prefix) lays these out CONTIGUOUSLY at the
    /// aligned tail `[S..S+u_structs]` (`S` = `prev.prefix_struct_def_count()`);
    /// the caller guarantees the live VM holds exactly `S` struct defs, so each
    /// installs at the `type_id` the delta baked into its `NewStruct` — no
    /// type-id relocation. Installed by `Vm::install_appended_types` BEFORE the
    /// new function bodies and the user main run (so a same-eval `NewStruct`
    /// resolves).
    pub new_struct_defs: Vec<crate::bytecode::StructDefInfo>,
    pub new_abstract_types: Vec<crate::bytecode::AbstractTypeDefInfo>,
    pub new_primitive_types: Vec<crate::bytecode::PrimitiveTypeDefInfo>,
    pub new_enum_defs: Vec<crate::bytecode::EnumDefInfo>,
    /// Exact typed source chronology of this delta's definition markers.
    pub definition_activations: Vec<ReplDefinitionActivation>,
    /// Inert runtime-conditional nominal templates present in `new_main`.
    /// Their registry identities are assigned only if execution reaches them.
    pub runtime_nominal_templates: Vec<crate::bytecode::DefineRuntimeNominalOperands>,
    /// Existing runtime-specialization rows whose fallback method is replaced
    /// when the keyed appended function activates. The VM applies these at the
    /// same source-order world as the method/caller group (Issue #9784).
    pub specializable_updates: Vec<(usize, SpecializableFunction)>,
    /// Contiguous specializable-table tail backed only by installed function
    /// bodies. Installed before the appended main so its numeric indices align
    /// with compiler bytecode; fallback visibility still follows method worlds.
    pub new_specializable_functions: Vec<SpecializableFunction>,
    /// The isolated user main, jumps relocated onto the live VM's code tail
    /// (AFTER any `new_functions` bodies), ready to hand to
    /// `Vm::reenter_appended_main`.
    pub new_main: Vec<crate::bytecode::Instr>,
    /// Source spans parallel to `new_main` (length-matched).
    pub new_source_map: Vec<Option<crate::span::Span>>,
    /// Brand-new module globals this delta binds, in slot order, to append to the
    /// live VM's frame-0 (`Vm::grow_global_slots`). Existing globals kept their
    /// live slot via the compile's seeding and are absent here.
    pub new_globals: Vec<String>,
    /// Main-scope binding names for the session's cross-eval global extraction
    /// (Issue #9157/#9182), same as the fresh path's `compiled.main_scope_names`.
    pub main_scope_names: HashSet<String>,
    /// Reusable compiler snapshot advanced to the exact live-VM layout after a
    /// definition append. Plain expression/global deltas leave this `None` so
    /// they do not copy an ever-growing bytecode prefix on every evaluation.
    pub next_persistent: Option<ReplPersistentCompile>,
}

/// One compiled, relocated new function ready to append to a live VM (Issue
/// #9199 LV3). `body` is the function's bytecode with intra-body jumps already
/// relocated onto the live code tail; `info.entry`/`code_start`/`code_end` are
/// the final live positions; `source` is the parallel span map.
#[derive(Debug)]
pub struct AppendableFunction {
    pub info: crate::bytecode::FunctionInfo,
    pub body: Vec<crate::bytecode::Instr>,
    pub source: Vec<Option<crate::span::Span>>,
}

/// Relocatable-delta compile for the REPL live-append path (Issue #9199 LV2 —
/// the relocatable-delta compiler contract).
///
/// Compiles ONLY the new `input` against the accumulated program (like
/// [`repl_delta_compile`]) BUT with `global_slot_seed = Some(live_frame0_names)`
/// so the main block's global slots align with the live VM's frame-0, and with
/// the base/user seam fusion barrier so the user main is a self-contained,
/// extractable block. It then slices `compiled.code[user_main_entry..]` (the
/// isolated user main — no base-main prefix, no accumulated-global re-inits),
/// relocates that main's intra-block jumps from the compiled buffer onto the live
/// VM's code tail (`live_code_len`), and returns it ready to splice. Function
/// CALLS stay index-based (the reused prefix aligns with the live VM), struct
/// type-ids and global slots are already live-aligned — so the only relocation is
/// the jumps.
///
/// Returns `Ok(None)` — the caller must fall back to the fresh recompile path —
/// whenever the compile is NOT cleanly appendable:
/// - a newly referenced function body is not in the structurally verified
///   input-function front (for example a nested/generated helper interposed in
///   the trailing region);
/// - a preloaded-package body was spliced AFTER the user main (not the tail);
/// - the user-main boundary is missing/out of range;
/// - the seed was not preserved verbatim at the front of the global-slot layout.
///
/// Preconditions the caller guarantees: `input` defines no
/// structs/types/modules/macros/usings and no opaque `eval` — but MAY define
/// brand-new generic functions or a hard-scope anonymous callable (Issue #9199
/// LV3 / #11569), returned in [`AppendableDelta::new_functions`]; when it does,
/// the live VM must hold EXACTLY `prev.bundle.compiled.functions.len()` functions
/// so the new bodies' function indices are live-aligned. The live VM was built from
/// `prev.bundle.compiled` (so function/struct/slot index spaces align);
/// `global_slot_seed` is the live VM's `global_slot_names`; and `live_code_len`
/// is the live VM's current `code_len()`.
/// Extract the brand-new CONCRETE struct definitions a delta compile appended to
/// the reused prefix (Issue #9199 LV4), or `None` (⇒ the caller falls back to the
/// full recompile) when the append is NOT cleanly installable.
///
/// A concrete struct's `type_id` IS its index in `struct_defs`; the pipeline,
/// seeded from the prefix's `struct_defs` (length `prev_struct_count` = `S`),
/// appends the input's declared concrete structs CONTIGUOUSLY at `[S..S+u]`. The
/// soundness contract this enforces (registry-level fail-closed): every emittable
/// `NewStruct(tid)` has `tid < compiled.struct_defs.len()`, and the caller
/// installs EXACTLY the returned tail `[S..S+u]` at aligned live `type_id`s. So
/// the append is sound iff:
///   1. `all_struct_defs.len() == S + u` — the compile appended NO EXTRA struct
///      beyond the input's `u` declared ones (a lazily-instantiated parametric
///      struct would push the count past this, and its `type_id` would be
///      referenced but UNINSTALLED); and
///   2. each tail entry's NAME equals the input's declared struct in order —
///      nothing (a module struct, a differently-ordered registration) interposed.
/// Any violation ⇒ `None`. This is a pure, directly-tested function so the
/// soundness gate is unit-checkable in isolation from the compile pipeline.
fn extract_appended_struct_defs(
    all_struct_defs: &[StructDefInfo],
    prev_struct_count: usize,
    input_struct_names: &[&str],
) -> Option<Vec<StructDefInfo>> {
    let u = input_struct_names.len();
    if all_struct_defs.len() != prev_struct_count.checked_add(u)? {
        return None;
    }
    if u == 0 {
        return Some(Vec::new());
    }
    let tail = &all_struct_defs[prev_struct_count..];
    for (i, def) in tail.iter().enumerate() {
        if def.name != input_struct_names[i] {
            return None;
        }
    }
    Some(tail.to_vec())
}

fn extract_appended_abstract_types(
    all: &[crate::bytecode::AbstractTypeDefInfo],
    previous_count: usize,
    input_names: &[&str],
) -> Option<Vec<crate::bytecode::AbstractTypeDefInfo>> {
    if all.len() != previous_count.checked_add(input_names.len())? {
        return None;
    }
    let tail = all.get(previous_count..)?;
    if tail
        .iter()
        .zip(input_names)
        .any(|(definition, name)| definition.name != *name)
    {
        return None;
    }
    Some(tail.to_vec())
}

fn extract_appended_primitive_types(
    all: &[crate::bytecode::PrimitiveTypeDefInfo],
    previous_count: usize,
    input_names: &[&str],
) -> Option<Vec<crate::bytecode::PrimitiveTypeDefInfo>> {
    if all.len() != previous_count.checked_add(input_names.len())? {
        return None;
    }
    let tail = all.get(previous_count..)?;
    if tail
        .iter()
        .zip(input_names)
        .any(|(definition, name)| definition.name != *name)
    {
        return None;
    }
    Some(tail.to_vec())
}

fn collect_enum_def_infos(
    block: &crate::ir::core::Block,
    output: &mut Vec<crate::bytecode::EnumDefInfo>,
) {
    for statement in &block.stmts {
        match statement {
            crate::ir::core::Stmt::EnumDef { enum_def, .. } => {
                output.push(crate::bytecode::EnumDefInfo {
                    name: enum_def.name.clone(),
                    base_type: enum_def.base_type.clone(),
                    members: enum_def
                        .members
                        .iter()
                        .map(|member| (member.name.clone(), member.value))
                        .collect(),
                });
            }
            crate::ir::core::Stmt::Block(inner) => collect_enum_def_infos(inner, output),
            _ => {}
        }
    }
}

fn extract_appended_enum_defs(
    all: &[crate::bytecode::EnumDefInfo],
    previous_count: usize,
    input: &[crate::bytecode::EnumDefInfo],
) -> Option<Vec<crate::bytecode::EnumDefInfo>> {
    if all.len() != previous_count.checked_add(input.len())? {
        return None;
    }
    let tail = all.get(previous_count..)?;
    if tail != input {
        return None;
    }
    Some(tail.to_vec())
}

/// Resolve lowered IR functions to their final compiled fallback indices.
///
/// Base merging deliberately shifts every source `definition_order`, so whole
/// `Function` equality is not stable across this boundary. `ReplMethodIdentity`
/// is: it uses the canonical Julia method signature and excludes source spans
/// and positional indices. Candidate fallback indices remain final compiled
/// indices, and duplicate identities are consumed in compiled-index order.
fn repl_compiled_function_indices(
    compiled: &CompiledProgram,
    sources: &[Arc<Function>],
    first_appended_index: usize,
) -> Option<Vec<usize>> {
    let mut candidates = BTreeMap::<ReplMethodIdentity, BTreeSet<usize>>::new();
    for specializable in &compiled.specializable_functions {
        if specializable.fallback_index < first_appended_index {
            continue;
        }
        candidates
            .entry(ReplMethodIdentity::from_function(
                &specializable.name,
                &specializable.ir,
            ))
            .or_default()
            .insert(specializable.fallback_index);
    }
    let mut used = HashSet::new();
    let resolved = sources
        .iter()
        .map(|source| {
            let identity = ReplMethodIdentity::from_function(&source.name, source);
            let identity_candidates = candidates.get_mut(&identity)?;
            let index = identity_candidates.iter().copied().find(|index| {
                !used.contains(index)
                    && compiled
                        .functions
                        .get(*index)
                        .is_some_and(|info| info.name == source.name)
            })?;
            identity_candidates.remove(&index);
            let info = compiled.functions.get(index)?;
            if info.name != source.name || !used.insert(index) {
                return None;
            }
            Some(index)
        })
        .collect::<Option<Vec<_>>>()?;
    let source_identities: HashSet<_> = sources
        .iter()
        .map(|source| ReplMethodIdentity::from_function(&source.name, source))
        .collect();
    if source_identities.iter().any(|identity| {
        candidates
            .get(identity)
            .is_some_and(|indices| !indices.is_empty())
    }) {
        return None;
    }
    Some(resolved)
}

fn collect_repl_static_helper_targets(
    instructions: &[crate::bytecode::Instr],
    compiled: &CompiledProgram,
    first_appended_index: usize,
    targets: &mut HashSet<usize>,
) -> Option<()> {
    for instr in instructions {
        match instr {
            crate::bytecode::Instr::CreateClosure { func_name, .. } => {
                let mut matches = compiled
                    .functions
                    .iter()
                    .enumerate()
                    .skip(first_appended_index)
                    .filter_map(|(index, info)| (info.name == *func_name).then_some(index));
                let index = matches.next()?;
                if matches.next().is_some() {
                    return None;
                }
                targets.insert(index);
            }
            crate::bytecode::Instr::CreateResolvedClosure(operands) => {
                for index in &operands.candidate_indices {
                    if *index >= compiled.functions.len() {
                        return None;
                    }
                    if *index >= first_appended_index {
                        targets.insert(*index);
                    }
                }
            }
            crate::bytecode::Instr::MakeGenerator(operands) => match &operands.callable {
                crate::bytecode::GeneratorCallableSpec::FunctionIndex(index)
                | crate::bytecode::GeneratorCallableSpec::TupleSplatFunctionIndex(index) => {
                    if *index >= compiled.functions.len() {
                        return None;
                    }
                    if *index >= first_appended_index {
                        targets.insert(*index);
                    }
                }
                crate::bytecode::GeneratorCallableSpec::FilteredFunctionIndex {
                    map_func_index,
                    predicate_func_index,
                } => {
                    if *map_func_index >= compiled.functions.len()
                        || *predicate_func_index >= compiled.functions.len()
                    {
                        return None;
                    }
                    if *map_func_index >= first_appended_index {
                        targets.insert(*map_func_index);
                    }
                    if *predicate_func_index >= first_appended_index {
                        targets.insert(*predicate_func_index);
                    }
                }
            },
            _ => {}
        }
    }
    Some(())
}

fn repl_required_function_indices(
    compiled: &CompiledProgram,
    first_appended_index: usize,
    seeds: impl IntoIterator<Item = usize>,
    main: &[crate::bytecode::Instr],
) -> Option<HashSet<usize>> {
    let mut required = seeds.into_iter().collect::<HashSet<_>>();
    collect_repl_static_helper_targets(main, compiled, first_appended_index, &mut required)?;

    let mut scanned = HashSet::new();
    loop {
        let pending = required
            .iter()
            .copied()
            .find(|index| !scanned.contains(index));
        let Some(index) = pending else {
            return Some(required);
        };
        let info = compiled.functions.get(index)?;
        if info.code_start > info.code_end || info.code_end > compiled.code.len() {
            return None;
        }
        scanned.insert(index);
        collect_repl_static_helper_targets(
            &compiled.code[info.code_start..info.code_end],
            compiled,
            first_appended_index,
            &mut required,
        )?;
    }
}

pub(crate) fn repl_relocatable_delta_compile(
    prev: &ReplPersistentCompile,
    input: &Program,
    global_types: &HashMap<String, ValueType>,
    global_struct_names: &HashMap<String, String>,
    global_slot_seed: &[String],
    live_code_len: usize,
) -> CResult<Option<AppendableDelta>> {
    let mut input_enum_defs = Vec::new();
    collect_enum_def_infos(&input.main, &mut input_enum_defs);

    // A prior function can contain a baked undefined-name trap for a callee or
    // constructor, or a dynamic load for a value binding that did not yet have
    // nominal identity. The historical full-refresh path repaired that forward
    // reference by recompiling the caller (`LoadAny(T)` becomes
    // `PushDataType(T)`, for example). Advancing the snapshot without doing so
    // would freeze the stale instruction permanently, so retain the
    // conservative full recompile exactly when a newly introduced binding is
    // named by one of the reused prefix's unresolved reads/calls.
    let new_binding_names: HashSet<&str> = input
        .functions
        .iter()
        .map(|function| function.name.as_str())
        .chain(
            input
                .structs
                .iter()
                .map(|definition| definition.name.as_str()),
        )
        .chain(
            input
                .abstract_types
                .iter()
                .map(|definition| definition.name.as_str()),
        )
        .chain(
            input
                .primitive_types
                .iter()
                .map(|definition| definition.name.as_str()),
        )
        .chain(input_enum_defs.iter().flat_map(|definition| {
            std::iter::once(definition.name.as_str()).chain(
                definition
                    .members
                    .iter()
                    .map(|(member_name, _)| member_name.as_str()),
            )
        }))
        .collect();
    if prev.bundle.compiled.code.iter().any(|instr| match instr {
        crate::bytecode::Instr::ThrowUndefVarError(name)
        | crate::bytecode::Instr::LoadAny(name)
        | crate::bytecode::Instr::ProbeRuntimeBinding(name)
        | crate::bytecode::Instr::LoadGlobalAny(name) => new_binding_names.contains(name.as_str()),
        _ => false,
    }) {
        return Ok(None);
    }

    let current_input_function_count = input.functions.len();
    let current_input_source_functions: Vec<Arc<Function>> = input
        .functions
        .iter()
        .filter(|function| !crate::compile::ir_inline::is_markerless_lowered_function(function))
        .cloned()
        .collect();
    let current_input_source_function_count = current_input_source_functions.len();
    let refresh_plan = prev.method_sources.refresh_plan_for(input);
    let mut compile_input = input.clone();
    compile_input
        .functions
        .extend(refresh_plan.methods.iter().cloned());

    let mut merged = if compile_input.base_function_count > 0 {
        compile_input.clone()
    } else {
        crate::compile::base_merge::merge_with_precompiled_base(&compile_input).program
    };
    merged.usings.extend(prev.usings.iter().cloned());
    let prev_function_count = prev.bundle.compiled.functions.len();

    let mut bundle = crate::compile::compile_core_program_internal(
        &merged,
        global_types,
        global_struct_names,
        crate::compile::CompilerCacheInput {
            precompiled_base: Some(&prev.bundle.compiled),
            method_tables: Some(&prev.bundle.method_tables),
            closure_captures: Some(&prev.bundle.closure_captures),
            inference_results: Some(&prev.bundle.inference_results),
            extra_imported_functions: Some(&prev.function_names),
            extra_inner_constructor_type_names: Some(&prev.inner_constructor_type_names),
            global_slot_seed: Some(global_slot_seed),
            // Prior modules' surface so `M.f()` / `M.const` in this delta resolve
            // against the live VM instead of erroring "Unknown module" (Issue
            // #9199 LV5). `None` for a module-free session.
            extra_module_metadata: (!prev.module_metadata.is_empty())
                .then_some(&prev.module_metadata),
            // Carry this input's Julia-visible source-method count into
            // source-order dispatch/activation. The pipeline selects that many
            // non-markerless methods structurally, so interposed helpers do not
            // consume the budget and dormant later methods cannot influence an
            // earlier body (Issues #9784/#11477).
            repl_current_function_count: Some(current_input_source_function_count),
            repl_current_struct_count: Some(input.structs.len()),
            // This path admits only Main-owned ordinary methods. Their mutation
            // cannot invalidate cached module/preload bodies through Julia's
            // module scoping, so keep those positional extras reused; otherwise
            // they interpose before the aligned live append region (#9784).
            repl_append_only_new_generics: true,
            ..Default::default()
        },
    )?;
    let Some(compiled_function_indices) = repl_compiled_function_indices(
        &bundle.compiled,
        &compile_input.functions,
        prev_function_count,
    ) else {
        return Ok(None);
    };
    let mut source_function_index_to_ordinal = HashMap::new();
    for (source_ordinal, (input_ordinal, _)) in input
        .functions
        .iter()
        .enumerate()
        .filter(|(_, function)| {
            !crate::compile::ir_inline::is_markerless_lowered_function(function)
        })
        .enumerate()
    {
        let Some(index) = compiled_function_indices.get(input_ordinal).copied() else {
            return Ok(None);
        };
        if source_function_index_to_ordinal
            .insert(index, source_ordinal)
            .is_some()
        {
            return Ok(None);
        }
    }
    if source_function_index_to_ordinal.len() != current_input_source_function_count {
        return Ok(None);
    }

    let mut refresh_body_overrides = HashMap::<
        usize,
        (
            crate::bytecode::FunctionInfo,
            usize,
            Vec<crate::bytecode::Instr>,
            Vec<Option<crate::span::Span>>,
        ),
    >::new();
    for (source_ordinal, refresh_ordinals) in &refresh_plan.refresh_ordinals_by_source {
        let active_refresh_ordinals: HashSet<usize> = refresh_ordinals.iter().copied().collect();
        let mut variant_input = compile_input.clone();
        let mut current_source_ordinal = 0usize;
        for (ordinal, function) in variant_input.functions.iter_mut().enumerate() {
            let active_current = if ordinal < current_input_function_count {
                if crate::compile::ir_inline::is_markerless_lowered_function(function) {
                    true
                } else {
                    let active = current_source_ordinal <= *source_ordinal;
                    current_source_ordinal += 1;
                    active
                }
            } else {
                false
            };
            let active_refresh = ordinal
                .checked_sub(current_input_function_count)
                .is_some_and(|refresh| active_refresh_ordinals.contains(&refresh));
            if !active_current && !active_refresh {
                Arc::make_mut(function).name =
                    format!("__repl_inactive_method_{}_{}", source_ordinal, ordinal);
            }
        }
        variant_input.main.stmts.clear();
        let mut variant_merged = if variant_input.base_function_count > 0 {
            variant_input.clone()
        } else {
            crate::compile::base_merge::merge_with_precompiled_base(&variant_input).program
        };
        variant_merged.usings.extend(prev.usings.iter().cloned());
        let variant = crate::compile::compile_core_program_internal(
            &variant_merged,
            global_types,
            global_struct_names,
            crate::compile::CompilerCacheInput {
                precompiled_base: Some(&prev.bundle.compiled),
                method_tables: Some(&prev.bundle.method_tables),
                closure_captures: Some(&prev.bundle.closure_captures),
                inference_results: Some(&prev.bundle.inference_results),
                extra_imported_functions: Some(&prev.function_names),
                extra_inner_constructor_type_names: Some(&prev.inner_constructor_type_names),
                global_slot_seed: Some(global_slot_seed),
                extra_module_metadata: (!prev.module_metadata.is_empty())
                    .then_some(&prev.module_metadata),
                repl_current_function_count: Some(0),
                repl_current_struct_count: Some(0),
                repl_append_only_new_generics: true,
                ..Default::default()
            },
        )?;
        let Some(variant_function_indices) = repl_compiled_function_indices(
            &variant.compiled,
            &variant_input.functions,
            prev_function_count,
        ) else {
            return Ok(None);
        };
        for refresh_ordinal in refresh_ordinals {
            let overall_ordinal = current_input_function_count + refresh_ordinal;
            let Some(index) = variant_function_indices.get(overall_ordinal).copied() else {
                return Ok(None);
            };
            if compiled_function_indices.get(overall_ordinal).copied() != Some(index) {
                return Ok(None);
            }
            let Some(info) = variant.compiled.functions.get(index) else {
                return Ok(None);
            };
            let (start, end) = (info.code_start, info.code_end);
            if start > end || end > variant.compiled.code.len() {
                return Ok(None);
            }
            let mut source = variant.compiled.source_map[start..end].to_vec();
            source.resize(end - start, None);
            refresh_body_overrides.insert(
                index,
                (
                    info.as_ref().clone(),
                    start,
                    variant.compiled.code[start..end].to_vec(),
                    source,
                ),
            );
        }
    }
    // ── Appendability gate: any failure ⇒ Ok(None) ⇒ caller falls back ──────
    // A preload splice appends bodies AFTER the user main.
    if bundle.preload_spliced_count != 0 {
        return Ok(None);
    }
    let Some(user_main_entry) = bundle.user_main_entry else {
        return Ok(None);
    };
    let code = &bundle.compiled.code;
    if user_main_entry > code.len() {
        return Ok(None);
    }
    // Program functions (source methods, lowered top-level helpers, and caller
    // refresh bodies) resolve through stable method identity to their final
    // specialization fallback indices. Main-inline helpers absent from
    // `Program.functions` are then discovered transitively from
    // closure/static-generator operands. No source-prefix or helper-name
    // ordering is semantic authority (Issue #9784).
    let Some(required_function_indices) = repl_required_function_indices(
        &bundle.compiled,
        prev_function_count,
        compiled_function_indices.iter().copied(),
        &code[user_main_entry..],
    ) else {
        return Ok(None);
    };
    let installed_threshold = required_function_indices
        .iter()
        .max()
        .and_then(|index| index.checked_add(1))
        .unwrap_or(prev_function_count);
    if installed_threshold < prev_function_count
        || (prev_function_count..installed_threshold)
            .any(|index| !required_function_indices.contains(&index))
    {
        // The VM append is positional. Never smuggle an unrelated re-lifted
        // Base duplicate through a gap merely to reach a later required helper.
        return Ok(None);
    }
    let new_fn_count = installed_threshold - prev_function_count;

    // Soundness gate. Every delta compile RE-LIFTS the ~34 "trailing lifted Base
    // closures" (broadcast `#sel`/`#fused`, curried `#…_curried`/`#…_eq_pred`,
    // `#__lambda_nested_*`, Issue #9254) as fresh duplicates because
    // `merge_with_precompiled_base` re-includes the Base source — so
    // `functions.len() > P` is NORMAL and NOT a rejection signal. The bodies of
    // those trailing dups (indices `≥ installed_threshold = P+u`) are NOT
    // installed on the live VM. So the append is sound only if the USER MAIN
    // references NO function at an index `≥ P+u`: it may call a base/prior
    // function (`< P`) or one of THIS delta's new functions (`[P, P+u)`), but a
    // reference into the trailing region (a base-closure dup, or a nested helper
    // whose body is not appended) rejects → the caller falls back
    // to the full recompile. `user_main_calls_only_existing_functions` is
    // conservative: any function-referencing instruction it can't confirm points
    // below the threshold rejects.
    let Some(installed_function_infos) = bundle.compiled.functions.get(..installed_threshold)
    else {
        return Ok(None);
    };
    let installed_function_names: HashSet<&str> = installed_function_infos
        .iter()
        .map(|function| function.name.as_str())
        .collect();
    let prev_specializable_count = prev.bundle.compiled.specializable_functions.len();
    if bundle.compiled.specializable_functions.len() < prev_specializable_count {
        return Ok(None);
    }
    let mut prior_specializable_by_identity = BTreeMap::<ReplMethodIdentity, Vec<usize>>::new();
    for (index, prior) in prev
        .bundle
        .compiled
        .specializable_functions
        .iter()
        .enumerate()
    {
        prior_specializable_by_identity
            .entry(ReplMethodIdentity::from_function(&prior.name, &prior.ir))
            .or_default()
            .push(index);
    }
    let new_specializable_final: Vec<SpecializableFunction> = bundle
        .compiled
        .specializable_functions
        .iter()
        .skip(prev_specializable_count)
        .take_while(|function| function.fallback_index < installed_threshold)
        .cloned()
        .collect();
    let new_specializable_functions: Vec<SpecializableFunction> = new_specializable_final
        .iter()
        .map(|fresh| {
            let identity = ReplMethodIdentity::from_function(&fresh.name, &fresh.ir);
            prior_specializable_by_identity
                .get(&identity)
                .and_then(|indices| indices.last())
                .and_then(|index| prev.bundle.compiled.specializable_functions.get(*index))
                .cloned()
                .unwrap_or_else(|| fresh.clone())
        })
        .collect();
    let mut safe_specializable_indices: HashSet<usize> = bundle
        .compiled
        .specializable_functions
        .iter()
        .zip(&prev.bundle.compiled.specializable_functions)
        .enumerate()
        .filter_map(|(index, (fresh, prior))| {
            (fresh.name == prior.name
                && fresh.fallback_index == prior.fallback_index
                && prior.fallback_index < prev_function_count)
                .then_some(index)
        })
        .collect();
    safe_specializable_indices.extend(
        prev_specializable_count..prev_specializable_count + new_specializable_functions.len(),
    );
    let mut all_specializable_by_identity = prior_specializable_by_identity.clone();
    for (offset, fresh) in new_specializable_final.iter().enumerate() {
        all_specializable_by_identity
            .entry(ReplMethodIdentity::from_function(&fresh.name, &fresh.ir))
            .or_default()
            .push(prev_specializable_count + offset);
    }
    let mut specializable_updates = Vec::new();
    for (ordinal, source) in compile_input.functions.iter().enumerate() {
        let identity = ReplMethodIdentity::from_function(&source.name, source);
        let Some(indices) = all_specializable_by_identity.get(&identity) else {
            continue;
        };
        let Some(fallback_index) = compiled_function_indices.get(ordinal).copied() else {
            return Ok(None);
        };
        for index in indices {
            let name = if *index < prev_specializable_count {
                &prev.bundle.compiled.specializable_functions[*index].name
            } else {
                &new_specializable_final[*index - prev_specializable_count].name
            };
            specializable_updates.push((
                *index,
                SpecializableFunction {
                    ir: Arc::clone(source),
                    name: name.clone(),
                    fallback_index,
                },
            ));
            safe_specializable_indices.insert(*index);
        }
    }
    if !user_main_calls_only_existing_functions(
        &code[user_main_entry..],
        installed_threshold,
        &safe_specializable_indices,
        &installed_function_names,
    ) {
        return Ok(None);
    }
    // Compatibility backstop: if any compiler path still lowers a top-level
    // hard-scope shadow through the legacy frame-0 scheme, it emits
    // `ForgetLetLocals([name])` at block exit — which would CLEAR the live global,
    // not the intended transient local (Issue #9199 LV2). `ForgetLetLocals` is the
    // sole legacy let-scope-clearing instruction, so scanning the user main for
    // one that names a seeded global remains a precise fail-closed reject. The
    // Issue #11569 lexical compiler emits `Enter/Load/Store/ExitLexicalScope`
    // instead and never reaches this compatibility path.
    let seed_set: HashSet<&str> = global_slot_seed.iter().map(String::as_str).collect();
    let forgets_live_global = code[user_main_entry..].iter().any(|instr| {
        matches!(
            instr,
            crate::bytecode::Instr::ForgetLetLocals(names)
                if names.iter().any(|n| seed_set.contains(n.as_str()))
        )
    });
    if forgets_live_global {
        return Ok(None);
    }
    // The seed (the live frame-0 layout) must be preserved VERBATIM at the front
    // of the compiled global-slot layout; the tail is the brand-new globals this
    // delta binds (LV2 frame-0 growth). A mismatch means the seeding assumption
    // broke — reject rather than risk a slot collision.
    let gsn = &bundle.compiled.global_slot_names;
    if gsn.len() < global_slot_seed.len() || gsn[..global_slot_seed.len()] != *global_slot_seed {
        return Ok(None);
    }
    let new_globals: Vec<String> = gsn[global_slot_seed.len()..].to_vec();
    let src = &bundle.compiled.source_map;

    // ── Extract + relocate the NEW user function bodies (Issue #9199 LV3) ──
    // Appended BEFORE the user main, in declaration order, at the live code tail;
    // `cursor` tracks the running live position. Each body's jumps are relocated
    // from its compiled position onto its live position; function-index operands
    // stay as-is (aligned — see the `installed_threshold` note above).
    let mut new_functions: Vec<AppendableFunction> = Vec::with_capacity(new_fn_count);
    let mut cursor = live_code_len;
    for i in 0..new_fn_count {
        let final_index = prev_function_count + i;
        if !required_function_indices.contains(&final_index) {
            return Ok(None);
        }
        let full_info = &bundle.compiled.functions[final_index];
        let (mut info, original_start, mut body, mut source) =
            if let Some((info, start, body, source)) = refresh_body_overrides.get(&final_index) {
                (info.clone(), *start, body.clone(), source.clone())
            } else {
                let (start, end) = (full_info.code_start, full_info.code_end);
                if start > end || end > code.len() {
                    return Ok(None);
                }
                let source = if end <= src.len() {
                    src[start..end].to_vec()
                } else {
                    Vec::new()
                };
                (
                    full_info.as_ref().clone(),
                    start,
                    code[start..end].to_vec(),
                    source,
                )
            };
        // Lowering helpers may still carry a generic-definition marker in their
        // nested IR. They are callable values, not source-visible generics; only
        // indices structurally mapped from non-zero definition provenance keep
        // publication markers (Issue #9784).
        for instr in &mut body {
            if matches!(instr, crate::bytecode::Instr::DefineEvalFunction(index)
                if !source_function_index_to_ordinal.contains_key(index))
            {
                *instr = crate::bytecode::Instr::Nop;
            }
        }
        // The body may reference only installable functions (`< installed_threshold`).
        if !user_main_calls_only_existing_functions(
            &body,
            installed_threshold,
            &safe_specializable_indices,
            &installed_function_names,
        ) {
            return Ok(None);
        }
        let body_len = body.len();
        let entry = cursor;
        super::relocate_jumps(&mut body, original_start, entry);
        source.resize(body_len, None);
        info.entry = entry;
        info.code_start = entry;
        info.code_end = entry + body_len;
        new_functions.push(AppendableFunction { info, body, source });
        cursor += body_len;
    }
    // ── Extract the NEW concrete struct definitions (Issue #9199 LV4) ──
    // A concrete struct's `type_id` IS its index in `struct_defs`; the pipeline
    // (seeded from the reused prefix's `struct_defs`, length `S`) appends the
    // input's new concrete structs CONTIGUOUSLY at the aligned tail
    // `[S..S+u_structs]`, followed by any lazily-instantiated parametric struct.
    // `Vm::install_appended_types` installs these at aligned `type_id`s, so a
    // `NewStruct(S+i, ..)` in a new body / the user main resolves with NO type-id
    // relocation (the caller guarantees the live VM holds exactly `S` struct
    // defs).
    //
    // SOUNDNESS (registry-level fail-closed): every emittable `NewStruct(tid)` has
    // `tid < compiled.struct_defs.len()`, and we install EXACTLY the tail
    // `[S..S+u_structs]`. So the append is sound iff (a) the compile appended NO
    // EXTRA struct beyond the input's declared concrete ones — i.e.
    // `compiled.struct_defs.len() == S + u_structs` (a lazily-instantiated
    // parametric type would push the count past this, and its `type_id` would be
    // uninstalled) — and (b) each tail entry's NAME matches the input's declared
    // struct in order (nothing interposed). Any mismatch ⇒ Ok(None) ⇒ the caller
    // falls back to the full recompile. Parametric / inner-constructor / redefined
    // structs are rejected upstream by the eligibility gate, but the count+name
    // check re-establishes the invariant independently here (a parametric struct
    // never lands in `struct_defs`, so it trips the count check).
    let prev_struct_count = prev.prefix_struct_def_count();
    let input_struct_names: Vec<&str> = input.structs.iter().map(|s| s.name.as_str()).collect();
    let Some(new_struct_defs) = extract_appended_struct_defs(
        &bundle.compiled.struct_defs,
        prev_struct_count,
        &input_struct_names,
    ) else {
        return Ok(None);
    };
    let input_abstract_names = input
        .abstract_types
        .iter()
        .map(|definition| definition.name.as_str())
        .collect::<Vec<_>>();
    let Some(new_abstract_types) = extract_appended_abstract_types(
        &bundle.compiled.abstract_types,
        prev.prefix_abstract_type_count(),
        &input_abstract_names,
    ) else {
        return Ok(None);
    };
    let input_primitive_names = input
        .primitive_types
        .iter()
        .map(|definition| definition.name.as_str())
        .collect::<Vec<_>>();
    let Some(new_primitive_types) = extract_appended_primitive_types(
        &bundle.compiled.primitive_types,
        prev.prefix_primitive_type_count(),
        &input_primitive_names,
    ) else {
        return Ok(None);
    };
    let Some(new_enum_defs) = extract_appended_enum_defs(
        &bundle.compiled.enum_defs,
        prev.prefix_enum_def_count(),
        &input_enum_defs,
    ) else {
        return Ok(None);
    };

    // The user main is spliced AFTER every new function body, so it is based at
    // `cursor` (== `live_code_len` when this delta defines no functions — the LV2
    // case).
    let main_base = cursor;

    // ── Slice out the isolated user main and relocate it onto the live tail ──
    let mut new_main: Vec<crate::bytecode::Instr> = code[user_main_entry..].to_vec();
    let mut new_source_map: Vec<Option<crate::span::Span>> = if user_main_entry <= src.len() {
        src[user_main_entry..].to_vec()
    } else {
        Vec::new()
    };
    new_source_map.resize(new_main.len(), None);
    // The user main's intra-block jumps are absolute in the compiled buffer
    // (based at `user_main_entry`); shift them onto the live VM's code tail after
    // the appended function bodies. Function CALLS are index-based (untouched).
    super::relocate_jumps(&mut new_main, user_main_entry, main_base);

    // Lowering-generated callable helpers can carry a nested
    // `DefineEvalFunction` even though they have no Julia source-definition
    // marker. Only functions with non-zero definition provenance participate in
    // the source activation sequence; helpers are installed immediately active.
    for instr in &mut new_main {
        if matches!(instr, crate::bytecode::Instr::DefineEvalFunction(index)
            if !source_function_index_to_ordinal.contains_key(index))
        {
            *instr = crate::bytecode::Instr::Nop;
        }
    }
    let mut next_enum_ordinal = 0usize;
    let mut enum_marker_mismatch = false;
    let mut function_marker_mismatch = false;
    let definition_activations: Vec<ReplDefinitionActivation> = new_main
        .iter()
        .filter_map(|instr| match instr {
            crate::bytecode::Instr::DefineEvalFunction(index) => {
                let Some(source_ordinal) = source_function_index_to_ordinal.get(index).copied()
                else {
                    function_marker_mismatch = true;
                    return None;
                };
                let mut refresh = Vec::new();
                for ordinal in refresh_plan
                    .refresh_ordinals_by_source
                    .get(&source_ordinal)
                    .into_iter()
                    .flatten()
                {
                    let Some(refresh_index) = compiled_function_indices
                        .get(current_input_function_count + ordinal)
                        .copied()
                    else {
                        function_marker_mismatch = true;
                        return None;
                    };
                    refresh.push(refresh_index);
                }
                if refresh.is_empty() {
                    Some(ReplDefinitionActivation::Function(*index))
                } else {
                    Some(ReplDefinitionActivation::FunctionGroup {
                        primary: *index,
                        refresh,
                    })
                }
            }
            crate::bytecode::Instr::DefineEvalStruct(type_id) => {
                Some(ReplDefinitionActivation::Struct(*type_id))
            }
            crate::bytecode::Instr::DefineEvalAbstractType(type_id) => {
                Some(ReplDefinitionActivation::AbstractType(*type_id))
            }
            crate::bytecode::Instr::DefineEvalPrimitiveType(type_id) => {
                Some(ReplDefinitionActivation::PrimitiveType(*type_id))
            }
            crate::bytecode::Instr::RegisterEnum(operands) => {
                let definition = new_enum_defs.get(next_enum_ordinal);
                if definition.is_none_or(|definition| {
                    definition.name != operands.type_name || definition.members != operands.members
                }) {
                    enum_marker_mismatch = true;
                    return None;
                }
                let index = prev.prefix_enum_def_count() + next_enum_ordinal;
                next_enum_ordinal += 1;
                Some(ReplDefinitionActivation::Enum(index))
            }
            _ => None,
        })
        .collect();
    if function_marker_mismatch || enum_marker_mismatch || next_enum_ordinal != new_enum_defs.len()
    {
        return Ok(None);
    }
    let runtime_nominal_templates = new_main
        .iter()
        .filter_map(|instruction| match instruction {
            crate::bytecode::Instr::DefineRuntimeNominal(operands) => Some((**operands).clone()),
            _ => None,
        })
        .collect::<Vec<_>>();
    let mut runtime_nominal_sites = HashSet::new();
    if runtime_nominal_templates
        .iter()
        .any(|template| !runtime_nominal_sites.insert(template.site_id))
    {
        return Ok(None);
    }
    let appended_function_end = prev_function_count + new_functions.len();
    let mut activation_members = HashSet::new();
    let mut source_function_indices = Vec::new();
    for activation in &definition_activations {
        match activation {
            ReplDefinitionActivation::Function(index) => {
                if !(prev_function_count..appended_function_end).contains(index)
                    || !activation_members.insert(*index)
                {
                    return Ok(None);
                }
                source_function_indices.push(*index);
            }
            ReplDefinitionActivation::FunctionGroup { primary, refresh } => {
                if refresh.is_empty()
                    || !(prev_function_count..appended_function_end).contains(primary)
                    || !activation_members.insert(*primary)
                    || refresh.iter().any(|index| {
                        !(prev_function_count..appended_function_end).contains(index)
                            || !activation_members.insert(*index)
                    })
                {
                    return Ok(None);
                }
                source_function_indices.push(*primary);
            }
            ReplDefinitionActivation::Struct(_)
            | ReplDefinitionActivation::AbstractType(_)
            | ReplDefinitionActivation::PrimitiveType(_)
            | ReplDefinitionActivation::Enum(_)
            | ReplDefinitionActivation::RuntimeNominal(_) => {}
        }
    }
    if source_function_indices.len() != current_input_source_functions.len()
        || source_function_indices
            .iter()
            .enumerate()
            .any(|(ordinal, index)| source_function_index_to_ordinal.get(index) != Some(&ordinal))
    {
        return Ok(None);
    }
    let source_functions = source_function_indices
        .iter()
        .copied()
        .zip(current_input_source_functions.iter().cloned())
        .collect::<Vec<_>>();
    let mut method_rows_by_index = HashMap::new();
    for (table_name, rows) in &bundle.source_ordered_method_sigs {
        for row in rows {
            if activation_members.contains(&row.sig.global_index)
                && method_rows_by_index
                    .insert(row.sig.global_index, (table_name.clone(), row.sig.clone()))
                    .is_some()
            {
                return Ok(None);
            }
        }
    }
    if method_rows_by_index.len() != activation_members.len() {
        return Ok(None);
    }
    let markerless_function_indices: HashSet<usize> = compile_input
        .functions
        .iter()
        .zip(&compiled_function_indices)
        .filter_map(|(function, index)| {
            crate::compile::ir_inline::is_markerless_lowered_function(function).then_some(*index)
        })
        .collect();
    let runtime_constructor_indices = runtime_nominal_templates
        .iter()
        .flat_map(|template| template.constructor_function_indices.iter().copied())
        .collect::<HashSet<_>>();
    for (offset, function) in new_functions.iter_mut().enumerate() {
        let index = prev_function_count + offset;
        // Activation members are installed dormant and become visible only at
        // their source marker. Runtime inner constructors follow their owning
        // nominal marker even though they are absent from the source-function
        // activation set. Every other marker-less lowering helper is callable
        // immediately, regardless of where it appears in the appended layout.
        function.info.min_world =
            initial_repl_append_world(index, &activation_members, &runtime_constructor_indices);
    }
    // Main-inline closure helpers have no source-level definition marker: their
    // fresh specialization row is installed already active with the helper.
    // Only source methods and caller-refresh members participate in world-age
    // keyed row replacement (Issue #9784 / regression #11569).
    specializable_updates.retain(|(_, update)| activation_members.contains(&update.fallback_index));

    let main_scope_names = bundle.compiled.main_scope_names.clone();

    let next_persistent = if new_functions.is_empty()
        && new_struct_defs.is_empty()
        && new_abstract_types.is_empty()
        && new_primitive_types.is_empty()
        && new_enum_defs.is_empty()
        && runtime_nominal_templates.is_empty()
    {
        None
    } else {
        // The fresh bundle owns the updated method tables, inference state, and
        // registries, but also contains deterministic re-lifted Base helpers that
        // the appendability gate intentionally did not install. Rebuild only its
        // positional program pieces from the verified relocated append so the
        // reusable prefix and live VM stay index-for-index identical.
        let mut advanced = bundle.compiled.clone();
        advanced.code = prev.bundle.compiled.code.clone();
        advanced.source_map = prev.bundle.compiled.source_map.clone();
        // Expression/global deltas deliberately do not advance the compiler
        // snapshot, but they do grow the live VM's code tail. Preserve that
        // positional gap with inert instructions before installing the next
        // definition body; no function entry points into those discarded mains.
        if advanced.code.len() > live_code_len {
            return Ok(None);
        }
        advanced
            .code
            .resize(live_code_len, crate::bytecode::Instr::Nop);
        advanced.source_map.resize(live_code_len, None);
        advanced.functions = prev.bundle.compiled.functions.clone();
        advanced.struct_defs = prev.bundle.compiled.struct_defs.clone();
        advanced.abstract_types = prev.bundle.compiled.abstract_types.clone();
        advanced.primitive_types = prev.bundle.compiled.primitive_types.clone();
        advanced.enum_defs = prev.bundle.compiled.enum_defs.clone();
        for function in &new_functions {
            advanced.code.extend_from_slice(&function.body);
            advanced.source_map.extend_from_slice(&function.source);
            let mut persistent_info = function.info.clone();
            let persistent_index = advanced.functions.len();
            // The live installer activates the method after insertion and bumps
            // its world age. This snapshot is committed only after that run
            // succeeds, so a future fresh VM must see the persisted method as an
            // already-active prefix method rather than retaining the delta
            // compiler's pre-activation `u64::MAX` sentinel.
            persistent_info.min_world = if runtime_constructor_indices.contains(&persistent_index) {
                u64::MAX
            } else {
                1
            };
            advanced.functions.push(std::rc::Rc::new(persistent_info));
        }
        advanced.struct_defs.extend(new_struct_defs.iter().cloned());
        advanced
            .abstract_types
            .extend(new_abstract_types.iter().cloned());
        advanced
            .primitive_types
            .extend(new_primitive_types.iter().cloned());
        advanced.enum_defs.extend(new_enum_defs.iter().cloned());
        if let Some(context) = advanced.compile_context.as_mut() {
            context.primitive_types = advanced.primitive_types.clone();
        }
        advanced.entry = advanced.code.len();
        advanced.code.extend_from_slice(&new_main);
        advanced.source_map.extend_from_slice(&new_source_map);
        advanced.specializable_functions = prev.bundle.compiled.specializable_functions.clone();
        for (index, update) in &specializable_updates {
            if let Some(row) = advanced.specializable_functions.get_mut(*index) {
                *row = update.clone();
            }
        }
        advanced
            .specializable_functions
            .extend(new_specializable_final.iter().cloned());

        bundle.compiled = advanced;
        let mut function_names = prev.function_names.clone();
        function_names.extend(
            source_functions
                .iter()
                .map(|(_, function)| function.name.clone()),
        );
        let type_names = collect_type_names(&bundle.compiled);
        let mut method_sources = prev.method_sources.clone();
        method_sources.apply(source_functions.iter().map(|(_, function)| function));
        let mut initial_specializable_functions =
            prev.bundle.compiled.specializable_functions.clone();
        initial_specializable_functions.extend(new_specializable_functions.iter().cloned());
        Some(ReplPersistentCompile {
            bundle,
            module_metadata: prev.module_metadata.clone(),
            usings: prev.usings.clone(),
            function_names,
            type_names,
            inner_constructor_type_names: prev.inner_constructor_type_names.clone(),
            method_sources,
            definition_transaction: (!source_functions.is_empty()).then(|| {
                let prior_method_tables = method_rows_by_index
                    .values()
                    .map(|(table_name, _)| table_name)
                    .collect::<HashSet<_>>()
                    .into_iter()
                    .map(|table_name| {
                        (
                            table_name.clone(),
                            prev.bundle.method_tables.get(table_name).cloned(),
                        )
                    })
                    .collect();
                ReplDefinitionTransaction {
                    prior_function_names: prev.function_names.clone(),
                    prior_method_sources: prev.method_sources.clone(),
                    prior_method_tables,
                    method_rows_by_index,
                    markerless_function_indices,
                    source_functions,
                    initial_specializable_functions,
                    specializable_updates: specializable_updates.clone(),
                }
            }),
        })
    };

    Ok(Some(AppendableDelta {
        new_functions,
        source_function_indices,
        new_struct_defs,
        new_abstract_types,
        new_primitive_types,
        new_enum_defs,
        definition_activations,
        runtime_nominal_templates,
        specializable_updates,
        new_specializable_functions,
        new_main,
        new_source_map,
        new_globals,
        main_scope_names,
        next_persistent,
    }))
}

fn initial_repl_append_world(
    function_index: usize,
    activation_members: &HashSet<usize>,
    runtime_constructor_indices: &HashSet<usize>,
) -> u64 {
    if activation_members.contains(&function_index)
        || runtime_constructor_indices.contains(&function_index)
    {
        u64::MAX
    } else {
        1
    }
}

#[cfg(test)]
const FALLBACK_BEARING_INSTRS: &[&str] = &[
    "CallDynamic",
    "CallTypedDispatch",
    "CallDynamicBinary",
    "CallStaticParametric",
];

#[cfg(test)]
fn static_parametric_test_instr(primary: usize, fallback: usize) -> crate::bytecode::Instr {
    crate::bytecode::Instr::CallStaticParametric(Box::new(crate::bytecode::StaticParametricCall {
        func_index: primary,
        arg_count: 1,
        bindings: Vec::new(),
        forward_caller_type_bindings: false,
        validate_argument_types: true,
        validation_fallback: Some(crate::bytecode::StaticParametricFallback {
            func_index: fallback,
            bindings: Vec::new(),
        }),
        runtime_binding_names: Vec::new(),
    }))
}

#[cfg(test)]
mod cache_issue_10969_tests;

#[cfg(test)]
mod cache_issue_10969_regression {
    #[test]
    fn cached_base_parametric_inner_origin_normalizes_rational_10969() -> Result<(), String> {
        super::cache_issue_10969_tests::cached_base_parametric_inner_origin_normalizes_rational_10969()
    }
}

#[cfg(test)]
mod repl_hof_helper_9784_tests {
    use super::*;
    use crate::bytecode::Instr;

    /// Base merging shifts every definition-order marker, so final compiled IR
    /// is not byte-for-byte equal to the original REPL fragment. Mapping must
    /// use stable method identity with interposed helpers (Issue #9784).
    #[test]
    fn repl_compiled_indices_ignore_shifted_definition_order_9784() -> Result<(), String> {
        let program = crate::pipeline::parse_and_lower(
            "map(i -> i + 1, [1]); f_9784(x) = x + 1; \
             ntuple(i -> i, 1); g_9784(x) = x + 2",
        )
        .map_err(|error| format!("parse/lower failed: {error:?}"))?;
        let mut output = crate::compile::compile_core_program_internal(
            &program,
            &HashMap::new(),
            &HashMap::new(),
            crate::compile::CompilerCacheInput::default(),
        )
        .map_err(|error| format!("compile failed: {error:?}"))?;
        for specializable in &mut output.compiled.specializable_functions {
            Arc::make_mut(&mut specializable.ir).span.definition_order += 10_000;
        }
        let sources = program
            .functions
            .iter()
            .filter(|function| matches!(function.name.as_str(), "f_9784" | "g_9784"))
            .cloned()
            .collect::<Vec<_>>();

        let indices = repl_compiled_function_indices(&output.compiled, &sources, 0)
            .ok_or_else(|| "shifted chronology destroyed source-to-index identity".to_string())?;
        assert_eq!(indices.len(), sources.len());
        assert_eq!(
            indices.iter().copied().collect::<HashSet<_>>().len(),
            indices.len()
        );
        Ok(())
    }

    #[test]
    fn repl_required_functions_discovers_nested_helpers_transitively_9784() -> Result<(), String> {
        let program = crate::pipeline::parse_and_lower(
            "helper_outer_9784(x) = x + 1\nhelper_inner_9784(x) = x + 2",
        )
        .map_err(|error| format!("parse/lower failed: {error:?}"))?;
        let mut output = crate::compile::compile_core_program_internal(
            &program,
            &HashMap::new(),
            &HashMap::new(),
            crate::compile::CompilerCacheInput::default(),
        )
        .map_err(|error| format!("compile failed: {error:?}"))?;
        let outer = output
            .compiled
            .functions
            .iter()
            .position(|function| function.name == "helper_outer_9784")
            .ok_or_else(|| "missing outer helper index".to_string())?;
        let inner = output
            .compiled
            .functions
            .iter()
            .position(|function| function.name == "helper_inner_9784")
            .ok_or_else(|| "missing inner helper index".to_string())?;
        let outer_name = output.compiled.functions[outer].name.clone();
        let inner_name = output.compiled.functions[inner].name.clone();
        output.compiled.code = vec![
            Instr::CreateClosure {
                func_name: inner_name,
                capture_names: Vec::new(),
            },
            Instr::Nop,
            Instr::CreateClosure {
                func_name: outer_name,
                capture_names: Vec::new(),
            },
        ];
        {
            let info = std::rc::Rc::make_mut(&mut output.compiled.functions[outer]);
            info.code_start = 0;
            info.code_end = 1;
        }
        {
            let info = std::rc::Rc::make_mut(&mut output.compiled.functions[inner]);
            info.code_start = 1;
            info.code_end = 2;
        }
        let first = outer.min(inner);
        let mut direct = HashSet::new();
        collect_repl_static_helper_targets(
            &output.compiled.code[2..],
            &output.compiled,
            first,
            &mut direct,
        )
        .ok_or_else(|| "direct helper target was rejected".to_string())?;
        assert_eq!(direct, HashSet::from([outer]));

        let filtered = Instr::MakeGenerator(Box::new(crate::bytecode::MakeGeneratorOperands {
            callable: crate::bytecode::GeneratorCallableSpec::FilteredFunctionIndex {
                map_func_index: 0,
                predicate_func_index: outer,
            },
            result_element_type: None,
        }));
        let mut filtered_targets = HashSet::new();
        collect_repl_static_helper_targets(
            std::slice::from_ref(&filtered),
            &output.compiled,
            first,
            &mut filtered_targets,
        )
        .ok_or_else(|| "installed-prefix map with appended predicate was rejected".to_string())?;
        assert_eq!(filtered_targets, HashSet::from([outer]));

        let required = repl_required_function_indices(
            &output.compiled,
            first,
            std::iter::empty(),
            &output.compiled.code[2..],
        )
        .ok_or_else(|| "recursive helper closure was rejected".to_string())?;
        assert_eq!(required, HashSet::from([outer, inner]));
        Ok(())
    }
}

#[cfg(test)]
#[allow(clippy::unwrap_used, clippy::expect_used)]
mod tests {
    use super::*;
    use crate::builtins::BuiltinId;
    use crate::bytecode::{
        CompiledProgram, Instr, SpecializableFunction, StructDefInfo, ValueType,
    };
    use crate::compile::promotion;
    use crate::ir::core::{Block, Expr, Function, KwParam, Literal, Module, Stmt, TypeAliasDef};
    use crate::span::Span;
    use crate::types::JuliaType;

    pub(super) fn parse_and_lower_ok(src: &str) -> Program {
        crate::pipeline::parse_and_lower(src).expect("pipeline error")
    }

    #[test]
    fn runtime_inner_constructor_append_world_is_dormant_11679() {
        let activation_members = HashSet::from([10]);
        let runtime_constructor_indices = HashSet::from([11]);
        assert_eq!(
            initial_repl_append_world(10, &activation_members, &runtime_constructor_indices),
            u64::MAX
        );
        assert_eq!(
            initial_repl_append_world(11, &activation_members, &runtime_constructor_indices),
            u64::MAX
        );
        assert_eq!(
            initial_repl_append_world(12, &activation_members, &runtime_constructor_indices),
            1
        );
    }

    #[test]
    fn cached_base_reuses_non_prefix_prelude_functions_issue_10211() {
        let program = parse_and_lower_ok("println(\"Hello World\")");
        let base_cache = get_or_init_base_cache().expect("base cache");
        let preload_cache_handle = crate::compile::preload_cache::get_or_init_preload_cache();

        let output = crate::compile::compile_core_program_internal(
            &program,
            &HashMap::new(),
            &HashMap::new(),
            crate::compile::CompilerCacheInput {
                precompiled_base: Some(&base_cache.compiled),
                method_tables: Some(&base_cache.method_tables),
                closure_captures: Some(&base_cache.closure_captures),
                inference_results: Some(&base_cache.inference_results),
                preload_cache: preload_cache_handle.as_ref().map(|c| &c.modules),
                preload_closure_layout: preload_cache_handle
                    .as_ref()
                    .map(|c| c.closure_layout.as_slice()),
                ..Default::default()
            },
        )
        .expect("cached compile");

        assert!(
            output.cached_base_extra_reused_count >= 100,
            "expected cached compile to reuse the non-prefix Base/prelude helpers from Issue #10211, got {}",
            output.cached_base_extra_reused_count
        );
    }

    #[test]
    fn compiled_type_object_extension_keeps_exact_param_julia_type_issue_10782() {
        clear_program_cache();
        let program = parse_and_lower_ok(
            r#"
struct LayoutPredicateDispatchBox3911
    n::Int64
end

Base.isbitstype(::Type{LayoutPredicateDispatchBox3911}) = false
"#,
        );
        let compiled = compile_with_cache(&program).expect("compile type-object extension");
        let method = compiled
            .functions
            .iter()
            .rev()
            .find(|func| func.name == "isbitstype")
            .expect("user isbitstype method should be compiled");

        assert_eq!(
            method.param_julia_types,
            vec![JuliaType::TypeOf(Box::new(JuliaType::Struct(
                "LayoutPredicateDispatchBox3911".to_string()
            )))],
            "compiled FunctionInfo must retain the exact Type{{LayoutPredicateDispatchBox3911}} dispatch signature"
        );
    }

    /// Every `Instr` variant that carries a function-table index or lifts/creates a
    /// function/closure. The live-append eligibility gate
    /// (`user_main_calls_only_existing_functions`) MUST classify each of these as
    /// either index-checked (`< threshold`) or an unconditional reject — never let
    /// one reach the `_ => true` catch-all, which would splice a delta main that
    /// dispatches to a function the held live VM never installed (Issue #9199 review
    /// r3535721788). Kept in sync with `Instr` by
    /// `live_append_gate_function_bearing_classification_is_fail_closed_9199`.
    const FUNCTION_BEARING_INSTRS: &[&str] = &[
        // Direct / resolved function-index calls (index-checked).
        "Call",
        "CallInbounds",
        "CallWithKwargs",
        "CallWithKwargsSplat",
        "CallWithSplat",
        "CallResolved",
        "CallResolvedI64Slots",
        "CallInboundsI64Slots",
        "CallStaticParametric",
        // Dynamic dispatch candidate lists (index-checked).
        "CallDynamic",
        "CallDynamicBinary",
        "CallDynamicBinaryBoth",
        "CallDynamicBinaryNoFallback",
        "CallDynamicOrBuiltin",
        "CallTypedDispatch",
        "CallParametricConstructorDispatch",
        "CallTypedDispatchOrBuiltin",
        "CallTypedDispatchOrBuiltinResult",
        "CallTypedDispatchOrBuiltinStoreDict",
        "CallTypedDispatchOrBuiltinStoreDictResult",
        "PushResolvedFunction",
        "IterateDynamic",
        // Direct-index HOF forms that immediately call a resolved function
        // (index-checked) — the review's NtupleFunc / SprintFunc miss.
        "NtupleFunc",
        "SprintFunc",
        // Runtime specialization (checked against its separate prefix table)
        // and runtime callable consumers (safe: no baked function index).
        "CallSpecialize",
        "CallSpecializeInbounds",
        "CallSpecializeI64Slots",
        "CallSpecializeInboundsI64Slots",
        "CallSpecializeF64Slots",
        "CallSpecializeInboundsF64Slots",
        "CallGlobalRef",
        "CallFunctionVariable",
        "CallFunctionVariableWithSplat",
        "CallFunctionVariableWithKwargsSplat",
        "InvokeFunctionVariable",
        "InvokeFunctionVariableWithKwargs",
        "InvokeFunctionVariableDynamicSignature",
        "InvokeFunctionVariableDynamicSignatureWithKwargs",
        // By-name/function definition forms reject; generator indices and
        // closure construction are checked against installed targets.
        "PushFunction",
        "CreateClosure",
        "CreateResolvedClosure",
        "MakeGenerator",
        "MakeGeneratorRuntime",
        "MakeGeneratorRuntimeFiltered",
        "DefineFunction",
        "DefineEvalFunction",
        // By-name runtime lookup guard for the splat/positional dynamic call
        // path (Issue #11320) — same conservative-reject shape as
        // `PushFunction`/`CallGlobalRef`.
        "RaiseUndefVarErrorIfFunctionInvisible",
    ];

    /// Regression for Issue #9199 review r3535721788: the live-append eligibility
    /// gate must REJECT a delta main that references a function whose body lives in
    /// the freshly-compiled (un-spliced) suffix — index ≥ threshold — via the
    /// HOF/closure opcodes `NtupleFunc` / `SprintFunc` / `IterateDynamic` /
    /// `CreateClosure`, which previously fell through the `_ => true` catch-all as
    /// silently safe. Splicing such a main would dispatch to a function index the
    /// held live VM never installed. Also pins that an IN-RANGE reference (a prior /
    /// Base function already in the VM) stays eligible, so the fix does not
    /// needlessly disable the fast path.
    #[test]
    fn live_append_gate_rejects_function_bearing_opcodes_9199() {
        use crate::bytecode::DynamicCallCandidate;
        const T: usize = 100; // live VM holds functions 0..100
        let fresh = 200usize; // an index ≥ threshold: NOT in the live VM
        let no_specializations = HashSet::new();
        let make_generator = |index| {
            Instr::MakeGenerator(Box::new(crate::bytecode::MakeGeneratorOperands {
                callable: crate::bytecode::GeneratorCallableSpec::FunctionIndex(index),
                result_element_type: None,
            }))
        };

        // Fresh function index ⇒ MUST be rejected (fall back to fresh recompile).
        let reject_fresh_index = [
            Instr::NtupleFunc(fresh),
            Instr::SprintFunc(fresh, 0),
            make_generator(fresh),
            Instr::IterateDynamic(1, vec![fresh]),
            // Pre-existing decoded call forms, as a sanity anchor.
            Instr::Call(fresh, 1),
            Instr::CallResolved(fresh, 1),
            Instr::call_dynamic("f", 0, 1, vec![DynamicCallCandidate::Method(fresh)]),
            Instr::CallDynamicBinary(0, 1, vec![fresh]),
            Instr::DefineEvalFunction(fresh),
            Instr::CreateResolvedClosure(Box::new(crate::bytecode::ResolvedClosureOperands {
                name: "#resolved#1".to_string(),
                capture_names: vec!["x".to_string()],
                candidate_indices: vec![fresh],
            })),
        ];
        for instr in reject_fresh_index {
            assert!(
                !user_main_calls_only_existing_functions(
                    std::slice::from_ref(&instr),
                    T,
                    &no_specializations,
                    &HashSet::new(),
                ),
                "{instr:?} references a function at index {fresh} ≥ threshold {T} \
                 (not in the live VM) and MUST be rejected"
            );
        }

        // Name-only closure creation is rejected when its body is absent from the
        // verified installed-name set. Other undecodable-reference forms remain
        // unconditionally rejected.
        let reject_always = [
            Instr::CreateClosure {
                func_name: "#closure#1".to_string(),
                capture_names: vec!["x".to_string()],
            },
            Instr::PushFunction("f".to_string()),
            Instr::DefineFunction(3),
            Instr::CallSpecialize(3, 1),
            Instr::CallGlobalRef(3),
        ];
        for instr in reject_always {
            assert!(
                !user_main_calls_only_existing_functions(
                    std::slice::from_ref(&instr),
                    10_000,
                    &no_specializations,
                    &HashSet::new(),
                ),
                "{instr:?} has no verified installed target and MUST be rejected"
            );
        }

        // In-range/indexed references and an exact installed closure name ⇒
        // eligible. The latter pins Issue #11569's hard-scope closure path.
        let installed_names = HashSet::from(["#closure#1"]);
        let safe_specializations = HashSet::from([5]);
        let accept_in_range = [
            Instr::NtupleFunc(5),
            Instr::SprintFunc(5, 0),
            make_generator(5),
            Instr::MakeGeneratorRuntime(false, None),
            Instr::MakeGeneratorRuntimeFiltered(None),
            Instr::IterateDynamic(1, vec![5, 7]),
            Instr::Call(5, 1),
            Instr::CallSpecialize(5, 1),
            Instr::CallFunctionVariable(1),
            Instr::InvokeFunctionVariable(1, vec!["Int64".to_string()]),
            Instr::call_dynamic("f", usize::MAX, 0, vec![DynamicCallCandidate::Method(5)]),
            Instr::DefineEvalFunction(5),
            Instr::CreateClosure {
                func_name: "#closure#1".to_string(),
                capture_names: vec!["x".to_string()],
            },
            Instr::CreateResolvedClosure(Box::new(crate::bytecode::ResolvedClosureOperands {
                name: "#resolved#1".to_string(),
                capture_names: vec!["x".to_string()],
                candidate_indices: vec![5],
            })),
        ];
        for instr in accept_in_range {
            assert!(
                user_main_calls_only_existing_functions(
                    std::slice::from_ref(&instr),
                    T,
                    &safe_specializations,
                    &installed_names,
                ),
                "{instr:?} references only verified installed functions and must stay eligible"
            );
        }

        // Non-function instructions never block appendability.
        let accept_non_function = [
            Instr::PushI64(1),
            Instr::AddI64,
            Instr::LoadGlobalAny("x".to_string()),
            Instr::DefineEvalStruct(7),
            Instr::CallBuiltin(BuiltinId::Compose, 2),
            Instr::ReturnAny,
        ];
        assert!(
            user_main_calls_only_existing_functions(
                &accept_non_function,
                0,
                &HashSet::new(),
                &HashSet::new(),
            ),
            "a main of purely non-function instructions must be appendable"
        );
    }

    /// Every Call-family `Instr` variant that carries a FALLBACK / target function
    /// index in addition to (or instead of) a candidate list — the operand the VM
    /// dispatches to when no candidate wins. The live-append gate must
    /// threshold-check that operand too, not just the candidate list (Issue #9199
    /// review r3536211262 / r3536211255). Pinned by
    /// `live_append_gate_rejects_out_of_range_fallback_operands_9199`; when you add
    /// a Call-family variant with a fallback/target function index, add it there.
    /// Operand-level fail-closed guard (Issue #9199 review r3536211262 /
    /// r3536211255 — the 3rd round of findings on this gate). The earlier guards
    /// pin function-bearing variant *names*; this one pins that each Call-family
    /// variant's FALLBACK/target function index is threshold-checked, not just its
    /// candidate list. For every such variant we build it with the fallback operand
    /// OUT of range (`>= threshold`) but every candidate IN range (`< threshold`)
    /// and assert the gate REJECTS it. Before the fix the candidate-only arms
    /// accepted these — splicing a delta that then dispatched on the live VM to a
    /// fallback body compiled only into the discarded fresh suffix. The positive
    /// controls (fallback in range) confirm the fix does not needlessly disable the
    /// fast path, and the candidate-out-of-range cases confirm the candidate check
    /// is still enforced.
    #[test]
    fn live_append_gate_rejects_out_of_range_fallback_operands_9199() {
        use crate::bytecode::DynamicCallCandidate;
        use strum::VariantNames;

        const T: usize = 100; // live VM holds functions 0..100
        let fresh = 200usize; // an index ≥ threshold: NOT in the live VM
        let old = 5usize; // an index < threshold: installed in the live VM

        // Sanity: the documented fallback-bearing names still exist on `Instr`.
        for name in FALLBACK_BEARING_INSTRS {
            assert!(
                Instr::VARIANTS.contains(name),
                "`Instr::{name}` is in FALLBACK_BEARING_INSTRS but no longer exists in \
                 Instr::VARIANTS — update the gate and this list together."
            );
        }

        // (A) Fallback operand OUT of range, candidates IN range ⇒ MUST reject.
        // This is the exact P1: a delta whose candidates are all old but whose
        // fallback is a fresh-suffix body the live append never installs.
        let reject_out_of_range_fallback = [
            // CallDynamic(fallback, argc, candidates) — 1st operand is the fallback.
            Instr::call_dynamic("f", fresh, 1, vec![DynamicCallCandidate::Method(old)]),
            // CallTypedDispatch(name, argc, fallback, candidates) — 3rd is the fallback.
            Instr::CallTypedDispatch("promote_rule".to_string(), 2, fresh, vec![old]),
            // CallDynamicBinary(fallback, check_pos, candidates) — 1st is the fallback.
            Instr::CallDynamicBinary(fresh, 0, vec![old]),
            static_parametric_test_instr(old, fresh),
        ];
        for instr in reject_out_of_range_fallback {
            assert!(
                !user_main_calls_only_existing_functions(
                    std::slice::from_ref(&instr),
                    T,
                    &HashSet::new(),
                    &HashSet::new(),
                ),
                "{instr:?} carries a fallback/target function index {fresh} ≥ threshold {T} \
                 (body only in the discarded fresh suffix) and MUST be rejected"
            );
        }

        // (B) Fallback IN range, candidates IN range ⇒ still eligible (no needless
        // disabling of the fast path — pure in-VM dynamic dispatch stays appendable).
        let accept_all_in_range = [
            Instr::call_dynamic("f", old, 1, vec![DynamicCallCandidate::Method(old)]),
            Instr::CallTypedDispatch("promote_rule".to_string(), 2, old, vec![old]),
            Instr::CallDynamicBinary(old, 0, vec![old]),
            static_parametric_test_instr(old, old),
        ];
        for instr in accept_all_in_range {
            assert!(
                user_main_calls_only_existing_functions(
                    std::slice::from_ref(&instr),
                    T,
                    &HashSet::new(),
                    &HashSet::new(),
                ),
                "{instr:?} references only installed functions (< {T}) and must stay eligible"
            );
        }

        // (C) Fallback IN range but a candidate OUT of range ⇒ still rejected
        // (proves the candidate check remains enforced alongside the fallback check).
        let reject_out_of_range_candidate = [
            Instr::call_dynamic("f", old, 1, vec![DynamicCallCandidate::Method(fresh)]),
            Instr::CallTypedDispatch("promote_rule".to_string(), 2, old, vec![fresh]),
            Instr::CallDynamicBinary(old, 0, vec![fresh]),
            static_parametric_test_instr(fresh, old),
        ];
        for instr in reject_out_of_range_candidate {
            assert!(
                !user_main_calls_only_existing_functions(
                    std::slice::from_ref(&instr),
                    T,
                    &HashSet::new(),
                    &HashSet::new(),
                ),
                "{instr:?} has a candidate at {fresh} ≥ threshold {T} and MUST be rejected"
            );
        }
    }

    /// LV3 (Issue #9199): the SAME operand-level gate now runs at the broadened
    /// threshold `installed_threshold = P + u` for a definition delta — accepting
    /// a function-index reference into the NEW batch `[P, P+u)` (a body calling a
    /// same-eval sibling, or the user main calling a just-defined function) while
    /// still rejecting a reference into the trailing re-lifted region
    /// `[P+u, Q)` (a base-closure dup or a lifted lambda whose body is NOT
    /// appended). This pins that the threshold broadening is exactly the operand
    /// admission LV3 relies on and nothing wider: a same-batch index is admitted
    /// ONLY because the threshold moved from `P` to `P+u`, and a trailing-region
    /// index is STILL rejected. Every Call-family fallback/target operand
    /// (`CallDynamic`/`CallTypedDispatch`/`CallDynamicBinary`) is checked against
    /// the SAME broadened threshold, so the operand-level fail-closed contract
    /// (r3536211262 / r3536211255) holds at `P+u` too.
    #[test]
    fn live_append_gate_lv3_new_batch_threshold_9199() {
        use crate::bytecode::DynamicCallCandidate::Method;

        const P: usize = 100; // prefix functions the live VM already holds
        const U: usize = 3; // this delta defines 3 new generics → [100, 103)
        const INSTALLED: usize = P + U; // 103
        let same_batch = P + 1; // 101 — a sibling being installed this eval
        let trailing_dup = INSTALLED + 5; // 108 — a re-lifted dup, NOT installed

        // A same-batch reference is admitted ONLY at the broadened threshold.
        let same_batch_refs = [
            Instr::Call(same_batch, 1),
            Instr::CallResolved(same_batch, 1),
            Instr::NtupleFunc(same_batch),
            Instr::call_dynamic("f", same_batch, 1, vec![Method(same_batch)]),
            Instr::CallTypedDispatch("f".to_string(), 1, same_batch, vec![same_batch]),
            Instr::CallDynamicBinary(same_batch, 0, vec![same_batch]),
        ];
        for instr in &same_batch_refs {
            assert!(
                user_main_calls_only_existing_functions(
                    std::slice::from_ref(instr),
                    INSTALLED,
                    &HashSet::new(),
                    &HashSet::new(),
                ),
                "{instr:?}: a same-batch index {same_batch} must be admitted at P+u={INSTALLED}"
            );
            assert!(
                !user_main_calls_only_existing_functions(
                    std::slice::from_ref(instr),
                    P,
                    &HashSet::new(),
                    &HashSet::new(),
                ),
                "{instr:?}: the same index {same_batch} must be REJECTED at the LV2 threshold P={P} \
                 — the admission is exactly the threshold broadening, nothing wider"
            );
        }

        // A trailing-region reference is STILL rejected at the broadened threshold
        // (its body is not installed) — including via a fallback/target operand.
        let trailing_refs = [
            Instr::Call(trailing_dup, 1),
            Instr::NtupleFunc(trailing_dup),
            Instr::call_dynamic("f", trailing_dup, 1, vec![Method(same_batch)]),
            Instr::CallTypedDispatch("f".to_string(), 1, trailing_dup, vec![same_batch]),
            Instr::CallDynamicBinary(trailing_dup, 0, vec![same_batch]),
        ];
        for instr in &trailing_refs {
            assert!(
                !user_main_calls_only_existing_functions(
                    std::slice::from_ref(instr),
                    INSTALLED,
                    &HashSet::new(),
                    &HashSet::new(),
                ),
                "{instr:?}: a trailing-region index {trailing_dup} ≥ P+u={INSTALLED} \
                 (not installed) MUST be rejected even at the broadened threshold"
            );
        }
    }

    /// LV4 (Issue #9199) registry-level fail-closed guard for the compiled-struct
    /// live-append. `extract_appended_struct_defs` is the sole gate that decides
    /// which `struct_defs` entries a delta installs; its soundness contract is
    /// "install EXACTLY the aligned tail `[S..S+u]` iff the compile appended no
    /// extra struct and the tail names the input's declared structs in order".
    /// This pins that contract directly: a clean append extracts the tail; an EXTRA
    /// struct (a lazily-instantiated parametric type whose `type_id` would be
    /// referenced but uninstalled), a MISSING struct, a NAME mismatch, or a WRONG
    /// ORDER each reject. Mirrors the operand-level fail-closed discipline the
    /// LV2/LV3 gates use (a count/name check at the registry level, not a variant
    /// scan) — see `memory/project/project_9199_lv2_live_append_gate_soundness.md`.
    #[test]
    fn lv4_struct_extraction_is_fail_closed_9199() {
        fn sdef(name: &str) -> StructDefInfo {
            StructDefInfo {
                name: name.to_string(),
                is_mutable: false,
                fields: Vec::new(),
                field_julia_types: Vec::new(),
                parent_type: None,
            }
        }
        // Clean append: prefix [A, B] (S=2), input defines [C] → tail [C].
        let all = [sdef("A"), sdef("B"), sdef("C")];
        assert_eq!(
            extract_appended_struct_defs(&all, 2, &["C"])
                .map(|v| v.iter().map(|d| d.name.clone()).collect::<Vec<_>>()),
            Some(vec!["C".to_string()]),
            "the aligned tail must extract cleanly"
        );
        // Clean multi-struct append preserves order.
        let all2 = [sdef("A"), sdef("C"), sdef("D")];
        assert_eq!(
            extract_appended_struct_defs(&all2, 1, &["C", "D"]).map(|v| v.len()),
            Some(2)
        );
        // No user structs + no extra append ⇒ empty (the LV2/LV3 path).
        assert_eq!(
            extract_appended_struct_defs(&[sdef("A"), sdef("B")], 2, &[]).map(|v| v.len()),
            Some(0)
        );

        // (1) EXTRA struct beyond the input's declared ones (a parametric
        // instantiation) ⇒ REJECT — its `type_id` would be referenced but never
        // installed.
        let extra = [sdef("A"), sdef("B"), sdef("C"), sdef("Complex{Int64}")];
        assert!(
            extract_appended_struct_defs(&extra, 2, &["C"]).is_none(),
            "an extra (uninstalled) struct in the compiled tail MUST reject"
        );
        // (2) An extra struct with NO declared user structs ⇒ REJECT.
        assert!(extract_appended_struct_defs(&[sdef("A"), sdef("B"), sdef("X")], 2, &[]).is_none());
        // (3) NAME mismatch (an interposed helper struct at the tail front) ⇒ REJECT.
        assert!(
            extract_appended_struct_defs(&[sdef("A"), sdef("B"), sdef("Helper")], 2, &["C"])
                .is_none(),
            "a tail whose name does not match the input's struct MUST reject"
        );
        // (4) WRONG ORDER ⇒ REJECT.
        assert!(
            extract_appended_struct_defs(&[sdef("A"), sdef("D"), sdef("C")], 1, &["C", "D"])
                .is_none(),
            "a tail in a different order than the input's structs MUST reject"
        );
        // (5) MISSING struct (a declared struct that did not land in struct_defs,
        // e.g. a parametric one) ⇒ REJECT via the count check.
        assert!(extract_appended_struct_defs(&[sdef("A"), sdef("C")], 1, &["C", "D"]).is_none());
    }

    include!("../../tests/internal/live_append_gate_9199_test.rs");

    /// Issue #8626: a persistent Base cache whose enum variant fingerprint
    /// does not match this build must be discarded (file removed) and the
    /// caller left to recompile from source — no panic, no misdecoded
    /// bytecode. This is the "detect → regenerate" fallback for enum
    /// declaration-order changes.
    #[test]
    fn mismatched_enum_fingerprint_cache_is_discarded_and_removed_8626() {
        let tampered = super::super::precompile::cache_bytes_with_tampered_enum_fingerprint();
        let dir = tempfile::tempdir().expect("create temp dir");
        let path = dir.path().join("sjulia_base_cache_test_8626.bin");
        fs::write(&path, &tampered).expect("write tampered cache");

        let result = read_persistent_base_cache(&path);
        assert!(
            result.is_none(),
            "a cache with a mismatched enum variant fingerprint must be rejected"
        );
        assert!(
            !path.exists(),
            "the stale cache file must be removed so it is regenerated"
        );
    }

    /// Issue #8718: a persistent Base cache built by a different compiler build
    /// must be discarded and removed so the caller regenerates from source.
    #[test]
    fn mismatched_compiler_build_cache_is_discarded_and_removed_8718() {
        let tampered =
            super::super::precompile::cache_bytes_with_tampered_compiler_build_fingerprint();
        let dir = tempfile::tempdir().expect("create temp dir");
        let path = dir.path().join("sjulia_base_cache_test_8718.bin");
        fs::write(&path, &tampered).expect("write tampered cache");

        let result = read_persistent_base_cache(&path);
        assert!(
            result.is_none(),
            "a cache with a mismatched compiler build fingerprint must be rejected"
        );
        assert!(
            !path.exists(),
            "the stale cache file must be removed so it is regenerated"
        );
    }

    fn test_span() -> Span {
        Span::new(0, 0, 1, 1, 1, 1)
    }

    fn empty_block() -> Block {
        Block {
            stmts: vec![],
            span: test_span(),
        }
    }

    fn type_alias(name: &str, target_type: &str, params: Vec<&str>) -> TypeAliasDef {
        TypeAliasDef {
            name: name.to_string(),
            target_type: target_type.to_string(),
            params: params.into_iter().map(str::to_string).collect(),
            span: test_span(),
        }
    }

    fn empty_function(name: &str) -> Function {
        Function {
            name: name.to_string(),
            params: vec![],
            kwparams: vec![],
            type_params: vec![],
            return_type: None,
            body: empty_block(),
            is_base_extension: false,
            is_runtime_eval: false,
            span: test_span(),
            new_struct_name: None,
        }
    }

    fn empty_module(name: &str, type_aliases: Vec<TypeAliasDef>) -> Module {
        Module {
            name: name.to_string(),
            is_bare: false,
            is_package_origin: false,
            is_base_origin: false,
            functions: vec![],
            structs: vec![],
            abstract_types: vec![],
            primitive_types: vec![],
            type_aliases,
            submodules: vec![],
            usings: vec![],
            macros: vec![],
            exports: vec![],
            publics: vec![],
            body: empty_block(),
            span: test_span(),
        }
    }

    fn minimal_program(type_aliases: Vec<TypeAliasDef>, modules: Vec<Module>) -> Program {
        Program {
            abstract_types: vec![],
            primitive_types: vec![],
            type_aliases,
            structs: vec![],
            functions: vec![],
            base_function_count: 0,
            modules,
            usings: vec![],
            macros: vec![],
            enums: vec![],
            main: empty_block(),
        }
    }

    fn minimal_compiled_program() -> CompiledProgram {
        CompiledProgram {
            code: vec![],
            source_map: vec![],
            functions: vec![],
            struct_defs: vec![],
            abstract_types: vec![],
            primitive_types: vec![],
            enum_defs: vec![],
            show_methods: vec![],
            print_methods: vec![],
            entry: 0,
            specializable_functions: vec![SpecializableFunction {
                ir: std::sync::Arc::new(empty_function("f")),
                name: "f".to_string(),
                fallback_index: 0,
            }],
            runtime_specialization_map: vec![],
            inference_global_types_snapshot: vec![],
            specialization_disable_flags: Default::default(),
            compile_context: None,
            base_function_count: 0,
            macro_bindings: std::collections::HashMap::new(),
            module_registry: Default::default(),
            global_slot_names: vec![],
            global_slot_types: vec![],
            global_slot_count: 0,
            main_scope_names: Default::default(),
        }
    }

    #[test]
    fn restore_compile_context_rehydrates_type_aliases_7955() {
        let program = minimal_program(
            vec![
                type_alias("TopAlias", "Int64", vec![]),
                type_alias("TopVec", "Vector{T}", vec!["T"]),
            ],
            vec![empty_module(
                "AliasModule",
                vec![type_alias("ModuleAlias", "Float64", vec![])],
            )],
        );
        let mut compiled = minimal_compiled_program();

        restore_compile_context_from_program(&mut compiled, &program);

        let ctx = compiled
            .compile_context
            .as_ref()
            .expect("compile context should be restored");
        assert_eq!(ctx.type_aliases.get("TopAlias"), Some(&"Int64".to_string()));
        assert_eq!(ctx.type_aliases.get("TopVec"), Some(&"Vector".to_string()));
        assert_eq!(ctx.type_aliases.get("ModuleAlias"), None);
        assert_eq!(
            ctx.type_aliases.get("AliasModule.ModuleAlias"),
            Some(&"Float64".to_string())
        );
    }

    #[test]
    fn restore_compile_context_rehydrates_alias_only_programs_7955() {
        let program = minimal_program(
            vec![],
            vec![empty_module(
                "AliasOnlyModule",
                vec![type_alias("ModuleAlias", "Float64", vec![])],
            )],
        );
        let mut compiled = minimal_compiled_program();
        compiled.specializable_functions.clear();

        restore_compile_context_from_program(&mut compiled, &program);

        let ctx = compiled
            .compile_context
            .as_ref()
            .expect("alias-only programs still carry compile context");
        assert_eq!(
            ctx.type_aliases.get("AliasOnlyModule.ModuleAlias"),
            Some(&"Float64".to_string())
        );
    }

    #[test]
    fn restore_compile_context_rehydrates_module_parametric_structs_7955() {
        let program = parse_and_lower_ok(
            r#"
            module ParametricStructModule7955
            struct Box{T}
                value::T
            end
            module Inner
            struct PairBox{T}
                value::T
            end
            end
            end
            "#,
        );
        let mut compiled = minimal_compiled_program();

        restore_compile_context_from_program(&mut compiled, &program);

        let ctx = compiled
            .compile_context
            .as_ref()
            .expect("compile context should be restored");
        assert!(ctx
            .parametric_structs
            .contains_key("ParametricStructModule7955.Box"));
        assert!(ctx
            .parametric_structs
            .contains_key("ParametricStructModule7955.Inner.PairBox"));
        // Issue #10337: module parametric structs are ALSO registered under
        // their bare name, mirroring the fresh pipeline's short-name aliasing
        // (`build_struct_tables`' "Also register with short name" branch).
        assert!(ctx.parametric_structs.contains_key("Box"));
        assert!(ctx.parametric_structs.contains_key("PairBox"));
    }

    #[test]
    fn restore_compile_context_rehydrates_parametric_struct_only_programs_7955() {
        let program = parse_and_lower_ok(
            r#"
            module ParametricOnlyModule7955
            struct Box{T}
                value::T
            end
            end
            "#,
        );
        let mut compiled = minimal_compiled_program();
        compiled.specializable_functions.clear();

        restore_compile_context_from_program(&mut compiled, &program);

        let ctx = compiled
            .compile_context
            .as_ref()
            .expect("parametric-struct-only programs still carry compile context");
        assert!(ctx
            .parametric_structs
            .contains_key("ParametricOnlyModule7955.Box"));
    }

    /// Permanent gate for Issues #6336/#6495: `core_signature` is the only
    /// stored type representation for `MethodSig`. The cold JuliaType accessors
    /// must be exactly the canonical inverse of that signature for every Base
    /// method, and the parameter-name side channel must stay arity-aligned.
    #[test]
    fn base_method_signature_accessors_are_canonical_issue_6495() {
        use crate::inference_core::{core_type_to_julia_type, CoreType};

        let base = get_or_init_base_cache().expect("base cache");
        let mut total = 0usize;
        let mut mismatches: Vec<String> = Vec::new();
        for (fname, table) in base.method_tables.iter() {
            for m in table.methods.iter() {
                let core = m.core_signature();
                let mut sig = &core;
                let mut type_var_count = 0usize;
                while let CoreType::UnionAll { body, .. } = sig {
                    type_var_count += 1;
                    sig = body;
                }
                let CoreType::Tuple(args) = sig else {
                    if mismatches.len() < 40 {
                        mismatches.push(format!("{fname}: non-tuple method signature {core:?}"));
                    }
                    continue;
                };
                if m.param_count() != args.len() && mismatches.len() < 40 {
                    mismatches.push(format!(
                        "{fname} (#{}): param_names len {} != core arg len {}",
                        m.global_index,
                        m.param_count(),
                        args.len()
                    ));
                }
                if m.core_signature_type_var_count() != type_var_count && mismatches.len() < 40 {
                    mismatches.push(format!(
                        "{fname} (#{}): type-var accessor {} != core wrappers {}",
                        m.global_index,
                        m.core_signature_type_var_count(),
                        type_var_count
                    ));
                }
                let accessor_row = m.projected_param_julia_types();
                let canonical_row: Vec<_> = args.iter().map(core_type_to_julia_type).collect();
                total += canonical_row.len();
                if accessor_row != canonical_row && mismatches.len() < 40 {
                    mismatches.push(format!(
                        "{fname} (#{}): accessor row diverges\n  accessor  {accessor_row:?}\n  canonical {canonical_row:?}",
                        m.global_index
                    ));
                }
            }
        }
        assert!(total > 5000, "base corpus unexpectedly small: {total}");
        assert!(
            mismatches.is_empty(),
            "MethodSig accessors diverge from canonical core_signature ({total} params checked):\n{}",
            mismatches.join("\n")
        );
    }

    /// Permanent gate for Issue #8549: production signatures are total. No
    /// method compiled into the Base + prelude corpus (the compiled Base
    /// cache is built from the full bundled prelude program) may carry a
    /// `Bottom` placeholder, a non-tuple body, or an arity-skewed canonical
    /// signature — the conservative dispatch fallbacks are reserved for the
    /// `#[cfg(test)]` placeholder constructors.
    #[test]
    fn base_and_prelude_method_tables_have_total_signatures_issue_8549() {
        let base = get_or_init_base_cache().expect("base cache");
        let mut total = 0usize;
        let mut defects: Vec<String> = Vec::new();
        for (fname, table) in base.method_tables.iter() {
            for m in table.methods.iter() {
                total += 1;
                if let Some(defect) = m.signature_defect() {
                    if defects.len() < 40 {
                        defects.push(format!("{fname} (#{}): {defect:?}", m.global_index));
                    }
                }
            }
        }
        assert!(
            total > 1000,
            "Base+prelude corpus unexpectedly small: {total} methods"
        );
        assert!(
            defects.is_empty(),
            "production method signatures must be total across the Base+prelude \
             corpus ({total} methods checked, Issue #8549):\n{}",
            defects.join("\n")
        );
    }

    /// Serde gate for Issue #6495: MethodTable round-trips must preserve only
    /// the canonical signature, display names, and dispatch metadata. No
    /// JuliaType projection is reconstructed into stored fields.
    #[test]
    fn base_method_tables_serde_roundtrip_preserves_canonical_signatures_issue_6495() {
        let base = get_or_init_base_cache().expect("base cache");
        for (fname, table) in base.method_tables.iter() {
            let bytes = bincode::serialize(table).expect("serialize method table");
            let restored: crate::compile::MethodTable =
                bincode::deserialize(&bytes).expect("deserialize method table");
            assert_eq!(restored.methods.len(), table.methods.len(), "{fname}");
            for (orig, rec) in table.methods.iter().zip(restored.methods.iter()) {
                assert_eq!(orig.param_names, rec.param_names, "{fname}: param names");
                assert_eq!(
                    orig.projected_param_julia_types(),
                    rec.projected_param_julia_types(),
                    "{fname}: canonical projected row"
                );
                assert_eq!(
                    orig.core_signature(),
                    rec.core_signature,
                    "{fname}: canonical signature"
                );
                assert_eq!(orig.global_index, rec.global_index, "{fname}");
                assert_eq!(orig.vararg_param_index, rec.vararg_param_index, "{fname}");
                assert_eq!(orig.vararg_fixed_count, rec.vararg_fixed_count, "{fname}");
            }
        }
    }

    fn projected_runtime_signature(
        m: &crate::compile::MethodSig,
        arg_len: usize,
    ) -> Option<Vec<String>> {
        let params = m.expanded_core_param_types_for_arity(arg_len)?;
        Some(
            params
                .iter()
                .map(crate::inference_core::core_type_to_julia_type)
                .map(|ty| ty.to_string())
                .collect(),
        )
    }

    /// The runtime derivation used by structured dynamic-call payloads must
    /// remain equal to the canonical MethodSig projection for the Base corpus.
    #[test]
    fn base_method_runtime_signature_derivation_uses_canonical_projection_issue_6495() {
        let base = get_or_init_base_cache().expect("base cache");
        let functions = &base.compiled.functions;
        let mut total = 0usize;
        let mut skipped_unmapped = 0usize;
        let mut mismatches: Vec<String> = Vec::new();
        for (fname, table) in base.method_tables.iter() {
            for m in table.methods.iter() {
                let Some(func) = functions.get(m.global_index) else {
                    skipped_unmapped += 1;
                    continue;
                };
                let table_base = fname.rsplit('.').next().unwrap_or(fname);
                let func_base = func
                    .name
                    .rsplit('.')
                    .next()
                    .and_then(|n| n.rsplit('#').next())
                    .unwrap_or(&func.name);
                if table_base != func_base {
                    skipped_unmapped += 1;
                    continue;
                }
                let arities: Vec<usize> = if let Some(vidx) = m.vararg_param_index {
                    if let Some(fixed) = m.vararg_fixed_count {
                        vec![vidx + fixed]
                    } else {
                        vec![vidx, vidx + 1, vidx + 3]
                    }
                } else {
                    vec![m.param_count()]
                };
                for arity in arities {
                    total += 1;
                    let projected = projected_runtime_signature(m, arity);
                    let derived = subset_julia_vm_bytecode::derived_runtime_signature(func, arity);
                    if projected != derived && mismatches.len() < 40 {
                        mismatches.push(format!(
                            "{fname} (#{} {}) arity {arity}:\n  projected {projected:?}\n  derived   {derived:?}",
                            m.global_index, func.name
                        ));
                    }
                }
            }
        }
        assert!(
            total > 4000,
            "base corpus unexpectedly small: {total} (skipped {skipped_unmapped})"
        );
        assert!(
            skipped_unmapped * 20 < total,
            "too many methods skipped as unmapped: {skipped_unmapped} of {total}"
        );
        assert!(
            mismatches.is_empty(),
            "FunctionInfo-derived runtime signatures diverge from canonical MethodSig projection ({total} signatures checked, {skipped_unmapped} skipped):\n{}",
            mismatches.join("\n")
        );
    }

    /// The collect candidate first parameter remains prefix-free when read via
    /// the canonical projection. If Base starts carrying a prefixed spelling,
    /// runtime derivation must adopt the same normalization.
    #[test]
    fn base_collect_candidate_names_need_no_normalization_issue_6496() {
        let base = get_or_init_base_cache().expect("base cache");
        let mut total = 0usize;
        let mut prefixed: Vec<String> = Vec::new();
        for fname in ["collect", "Base.collect"] {
            let Some(table) = base.method_tables.get(fname) else {
                continue;
            };
            for m in table.methods.iter() {
                if m.param_count() == 0 {
                    continue;
                }
                total += 1;
                let raw = m.projected_param_julia_type(0).to_string();
                let normalized = raw.replace("Base.Iterators.", "").replace("Base.", "");
                if raw != normalized {
                    prefixed.push(format!("{fname}: {raw}"));
                }
            }
        }
        assert!(total > 5, "collect corpus unexpectedly small: {total}");
        assert!(
            prefixed.is_empty(),
            "collect candidate first-param names are no longer prefix-free; runtime derivation must normalize them:\n{}",
            prefixed.join("\n")
        );
    }

    fn instructions_after_last_make_range(code: &[Instr]) -> &[Instr] {
        let idx = code
            .iter()
            .rposition(|instr| matches!(instr, Instr::MakeRangeLazy))
            .expect("compiled code should contain MakeRangeLazy");
        &code[idx..]
    }

    fn instructions_after_last_make_generator(code: &[Instr]) -> &[Instr] {
        let idx = code
            .iter()
            .rposition(|instr| {
                matches!(
                    instr,
                    Instr::MakeGenerator(..)
                        | Instr::MakeGeneratorRuntime(..)
                        | Instr::MakeGeneratorRuntimeFiltered(..)
                        | Instr::WrapInGenerator
                )
            })
            .expect("compiled code should contain a generator constructor");
        &code[idx..]
    }

    #[test]
    fn test_persistent_base_cache_opt_out_requires_one() {
        assert!(env_flag_is_enabled(Some("1")));
        assert!(!env_flag_is_enabled(None));
        assert!(!env_flag_is_enabled(Some("")));
        assert!(!env_flag_is_enabled(Some("0")));
        assert!(!env_flag_is_enabled(Some("true")));
    }

    #[test]
    fn test_should_skip_base_cache_when_user_replaces_base_signature() {
        let program = parse_and_lower_ok("identity(x) = x");
        let prelude = crate::get_prelude_program().expect("prelude must be loaded");
        assert!(program.base_function_count < prelude.functions.len());
        assert!(should_skip_base_cache_for_program(
            &program,
            prelude.functions.len()
        ));
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

    #[test]
    fn test_is_compiler_generated_anonymous_def_predicate_9250() {
        // Lifted arrows / do-blocks / generator bodies, bare and qualified.
        assert!(is_compiler_generated_anonymous_def("__lambda_0"));
        assert!(is_compiler_generated_anonymous_def("__do_block_3"));
        assert!(is_compiler_generated_anonymous_def("__gen_body_1"));
        assert!(is_compiler_generated_anonymous_def("__gen_pred_1"));
        assert!(is_compiler_generated_anonymous_def("outer#__lambda_2"));
        assert!(is_compiler_generated_anonymous_def("a#b#__gen_body_9"));
        // Genuinely named user methods (the #8469 case) are NOT excluded.
        assert!(!is_compiler_generated_anonymous_def("foo"));
        assert!(!is_compiler_generated_anonymous_def("identity"));
        assert!(!is_compiler_generated_anonymous_def("__not_a_known_prefix"));
        assert!(!is_compiler_generated_anonymous_def("outer#helper"));
    }

    #[test]
    fn test_should_not_skip_base_cache_for_anonymous_lambda_in_main_9250() {
        // Issue #9250: an anonymous function passed as a value (a HOF argument)
        // is lifted to a `__lambda_*` `Stmt::FunctionDef` in main, but it is a
        // value — never a named method reachable through a generic Base helper —
        // so it must stay on the fast cached-Base path (was ~25x slower).
        let program = parse_and_lower_ok("map(x -> x + 1, [1, 2, 3])");
        let prelude = crate::get_prelude_program().expect("prelude must be loaded");
        assert_eq!(program.base_function_count, prelude.functions.len());
        assert!(!should_skip_base_cache_for_program(
            &program,
            prelude.functions.len()
        ));
    }

    #[test]
    fn test_should_not_skip_base_cache_for_generator_in_main_9250() {
        // Issue #9250: a generator body (`__gen_body_*`) lifted into main is
        // driven by the iterator machinery, not resolved by name from Base.
        let program = parse_and_lower_ok("sum(x^2 for x in 1:3)");
        let prelude = crate::get_prelude_program().expect("prelude must be loaded");
        assert_eq!(program.base_function_count, prelude.functions.len());
        assert!(!should_skip_base_cache_for_program(
            &program,
            prelude.functions.len()
        ));
    }

    #[test]
    fn test_should_skip_base_cache_for_retry_closure_factory_9250() {
        // Issue #9250 / #8469: `retry` returns a closure capturing its kwparams
        // (`CreateClosure { capture_names: ["delays", "f", "check"] }`); the
        // cached Base bytecode cannot rewire that capture, so a program calling
        // it must keep the whole-program compile even though its only main-level
        // function value is an anonymous lambda (the #9250 narrowing must not
        // re-expose this hazard).
        let program = parse_and_lower_ok("g = retry(() -> 42)\ng()");
        let prelude = crate::get_prelude_program().expect("prelude must be loaded");
        assert_eq!(program.base_function_count, prelude.functions.len());
        assert!(should_skip_base_cache_for_program(
            &program,
            prelude.functions.len()
        ));
    }

    #[test]
    fn test_should_not_skip_base_cache_for_do_block_in_main_9250() {
        // Issue #9250: `do`-block anonymous function lifted into main.
        let program = parse_and_lower_ok("map([1, 2, 3]) do x\n    x + 1\nend");
        let prelude = crate::get_prelude_program().expect("prelude must be loaded");
        assert_eq!(program.base_function_count, prelude.functions.len());
        assert!(!should_skip_base_cache_for_program(
            &program,
            prelude.functions.len()
        ));
    }

    #[test]
    fn test_should_not_skip_base_cache_for_user_type_anchored_promote_rule_8555() {
        // Issue #8555: a promote_rule method anchored to a program-local type
        // no longer disables the Base cache; the cached bytecode's dispatch
        // candidate lists are refreshed post-merge instead.
        let program = parse_and_lower_ok(
            r#"
struct MyReal
    value::Float64
end

function promote_rule(::Type{MyReal}, ::Type{Float64})
    MyReal
end

promote_type(MyReal, Float64)
"#,
        );
        let prelude = crate::get_prelude_program().expect("prelude must be loaded");
        assert_eq!(program.base_function_count, prelude.functions.len());
        assert!(!should_skip_base_cache_for_program(
            &program,
            prelude.functions.len()
        ));
    }

    #[test]
    fn test_should_skip_base_cache_for_base_type_pirating_promote_rule_8555() {
        // A promote_rule method whose every slot can match Base-known types
        // (type piracy) can invalidate static promotion folds inside cached
        // Base bytecode, which no post-load patch reaches — keep the bypass.
        let program = parse_and_lower_ok(
            r#"
function promote_rule(::Type{Char}, ::Type{Bool})
    Int64
end

promote_type(Char, Bool)
"#,
        );
        let prelude = crate::get_prelude_program().expect("prelude must be loaded");
        assert_eq!(program.base_function_count, prelude.functions.len());
        assert!(should_skip_base_cache_for_program(
            &program,
            prelude.functions.len()
        ));
    }

    #[test]
    fn test_should_skip_base_cache_for_typevar_only_promote_rule_8555() {
        // A method with no program-local anchor (Base-known concrete type +
        // bare TypeVar slot) can capture Base-known pairs: conservative bypass.
        let program = parse_and_lower_ok(
            r#"
function promote_rule(::Type{Int16}, ::Type{S}) where {S<:Integer}
    Int64
end

promote_type(Int16, Int8)
"#,
        );
        let prelude = crate::get_prelude_program().expect("prelude must be loaded");
        assert_eq!(program.base_function_count, prelude.functions.len());
        assert!(should_skip_base_cache_for_program(
            &program,
            prelude.functions.len()
        ));
    }

    #[test]
    fn test_should_not_skip_base_cache_for_user_type_anchored_iterator_traits_8555() {
        // Issue #8555: iterator trait methods anchored to a program-local
        // type no longer disable the Base cache (#4088 bypass retired).
        let program = parse_and_lower_ok(
            r#"
struct MyIter8555 end

Base.IteratorSize(::Type{MyIter8555}) = Base.SizeUnknown()
Base.IteratorEltype(::Type{MyIter8555}) = Base.HasEltype()

Base.IteratorSize(MyIter8555)
"#,
        );
        let prelude = crate::get_prelude_program().expect("prelude must be loaded");
        assert_eq!(program.base_function_count, prelude.functions.len());
        assert!(!should_skip_base_cache_for_program(
            &program,
            prelude.functions.len()
        ));
    }

    #[test]
    fn test_should_skip_base_cache_for_base_type_pirating_iterator_traits_8555() {
        // An IteratorSize method over a Base-known type (piracy) can affect
        // static folds inside cached Base bytecode: keep the bypass.
        let program = parse_and_lower_ok(
            r#"
Base.IteratorSize(::Type{String}) = Base.SizeUnknown()

Base.IteratorSize(String)
"#,
        );
        let prelude = crate::get_prelude_program().expect("prelude must be loaded");
        assert_eq!(program.base_function_count, prelude.functions.len());
        assert!(should_skip_base_cache_for_program(
            &program,
            prelude.functions.len()
        ));
    }

    #[test]
    fn test_should_not_skip_base_cache_for_user_type_anchored_dict_views_8602() {
        // Issue #8602: keys/values/pairs methods anchored to a program-local
        // type no longer disable the Base cache (#4671 bypass retired); the
        // cached bytecode's dispatch candidate lists are refreshed post-merge.
        let program = parse_and_lower_ok(
            r#"
struct MyMap8602
    ks::Vector{Symbol}
end

Base.keys(m::MyMap8602) = m.ks
Base.values(m::MyMap8602) = m.ks
Base.pairs(m::MyMap8602) = m.ks

keys(MyMap8602([:a]))
"#,
        );
        let prelude = crate::get_prelude_program().expect("prelude must be loaded");
        assert_eq!(program.base_function_count, prelude.functions.len());
        assert!(!should_skip_base_cache_for_program(
            &program,
            prelude.functions.len()
        ));
    }

    #[test]
    fn test_should_skip_base_cache_for_base_type_pirating_dict_views_8602() {
        // A keys/values/pairs method over a Base-known Dict instantiation
        // (the exact piracy #4671 was filed about) can invalidate static
        // resolutions inside cached Base bytecode: keep the bypass.
        let program = parse_and_lower_ok(
            r#"
Base.values(d::Dict{String,Float64}) = 42

values(Dict("x" => 1.0))
"#,
        );
        let prelude = crate::get_prelude_program().expect("prelude must be loaded");
        assert_eq!(program.base_function_count, prelude.functions.len());
        assert!(should_skip_base_cache_for_program(
            &program,
            prelude.functions.len()
        ));
    }

    #[test]
    fn test_julia_type_program_local_anchor_classification_8555() {
        use crate::types::JuliaType;

        let user_ty = JuliaType::Struct("MyLocalType8555".to_string());
        let base_struct = JuliaType::Struct("Complex{Float64}".to_string());

        // Type{UserStruct} anchors; builtin and prelude-declared types do not.
        assert!(julia_type_requires_program_local_type(&JuliaType::TypeOf(
            Box::new(user_ty.clone())
        )));
        assert!(!julia_type_requires_program_local_type(&JuliaType::Int64));
        assert!(!julia_type_requires_program_local_type(&JuliaType::TypeOf(
            Box::new(JuliaType::Float64)
        )));
        assert!(!julia_type_requires_program_local_type(&base_struct));

        // A Union anchors only when every member is program-local.
        assert!(julia_type_requires_program_local_type(&JuliaType::Union(
            vec![user_ty.clone()]
        )));
        assert!(!julia_type_requires_program_local_type(&JuliaType::Union(
            vec![user_ty, JuliaType::Int64]
        )));
        assert!(!julia_type_requires_program_local_type(&JuliaType::Union(
            vec![]
        )));

        // Module-qualified names are conservatively Base-known.
        assert!(!julia_type_requires_program_local_type(&JuliaType::Struct(
            "SomeMod.Inner".to_string()
        )));
    }

    fn kwparam(name: &str) -> KwParam {
        KwParam::new(
            name.to_string(),
            Expr::Literal(Literal::Nothing, test_span()),
            None,
            test_span(),
        )
    }

    #[test]
    fn test_should_skip_base_cache_for_user_function_shadowing_base_kwparam_8469() {
        let mut base_retry = empty_function("retry");
        base_retry.kwparams = vec![kwparam("check")];

        let mut program = minimal_program(vec![], vec![]);
        program.functions = vec![
            std::sync::Arc::new(base_retry),
            std::sync::Arc::new(empty_function("check")),
        ];
        program.base_function_count = 1;

        assert!(program_user_functions_shadow_base_kwparams(&program));
        assert!(should_skip_base_cache_for_program(&program, 1));
    }

    #[test]
    fn test_should_not_skip_base_cache_for_non_shadowing_user_function_8469() {
        let mut base_retry = empty_function("retry");
        base_retry.kwparams = vec![kwparam("check")];

        let mut program = minimal_program(vec![], vec![]);
        program.functions = vec![
            std::sync::Arc::new(base_retry),
            std::sync::Arc::new(empty_function("not_check")),
        ];
        program.base_function_count = 1;

        assert!(!program_user_functions_shadow_base_kwparams(&program));
        assert!(!should_skip_base_cache_for_program(&program, 1));
    }

    fn testset_with_function(name: &str) -> Stmt {
        Stmt::TestSet {
            name: "issue_8469".to_string(),
            body: Block {
                stmts: vec![Stmt::FunctionDef {
                    func: Box::new(empty_function(name)),
                    span: test_span(),
                }],
                span: test_span(),
            },
            span: test_span(),
        }
    }

    #[test]
    fn test_should_skip_base_cache_for_block_function_shadowing_base_kwparam_8469() {
        let mut base_retry = empty_function("retry");
        base_retry.kwparams = vec![kwparam("check")];

        let mut program = minimal_program(vec![], vec![]);
        program.functions = vec![std::sync::Arc::new(base_retry)];
        program.base_function_count = 1;
        program.main.stmts = vec![testset_with_function("check")];

        assert!(program_user_functions_shadow_base_kwparams(&program));
        assert!(should_skip_base_cache_for_program(&program, 1));
    }

    #[test]
    fn test_kwparam_shadow_scanner_ignores_non_shadowing_block_function_8469() {
        let mut base_retry = empty_function("retry");
        base_retry.kwparams = vec![kwparam("check")];

        let mut program = minimal_program(vec![], vec![]);
        program.functions = vec![std::sync::Arc::new(base_retry)];
        program.base_function_count = 1;
        program.main.stmts = vec![testset_with_function("not_check")];

        assert!(!program_user_functions_shadow_base_kwparams(&program));
    }

    fn letblock_expr_with_function(name: &str) -> Stmt {
        Stmt::Expr {
            expr: Expr::LetBlock {
                bindings: vec![],
                body: Block {
                    stmts: vec![Stmt::FunctionDef {
                        func: Box::new(empty_function(name)),
                        span: test_span(),
                    }],
                    span: test_span(),
                },
                span: test_span(),
            },
            span: test_span(),
        }
    }

    #[test]
    fn test_should_skip_base_cache_for_letblock_function_shadowing_base_kwparam_8469() {
        let mut base_retry = empty_function("retry");
        base_retry.kwparams = vec![kwparam("check")];

        let mut program = minimal_program(vec![], vec![]);
        program.functions = vec![std::sync::Arc::new(base_retry)];
        program.base_function_count = 1;
        program.main.stmts = vec![letblock_expr_with_function("check")];

        assert!(program_user_functions_shadow_base_kwparams(&program));
        assert!(should_skip_base_cache_for_program(&program, 1));
    }

    #[test]
    fn test_should_skip_base_cache_for_user_main_block_function_def_issue_8469() {
        let program = parse_and_lower_ok(
            r#"
using Test

@testset "block local function" begin
    function check(x, y)
        x == y
    end
    @test check(1, 1)
end
"#,
        );
        let prelude = crate::get_prelude_program().expect("prelude must be loaded");
        assert_eq!(program.base_function_count, prelude.functions.len());
        assert!(program_main_contains_block_function_defs(&program));
        assert!(should_skip_base_cache_for_program(
            &program,
            prelude.functions.len()
        ));
    }

    #[test]
    fn test_should_skip_base_cache_for_main_user_function_ref_issue_8469() {
        let mut base_retry = empty_function("retry");
        base_retry.kwparams = vec![kwparam("check")];
        let mut main_anon = empty_function("main#anon");
        main_anon.span.definition_order = 1;

        let mut program = minimal_program(vec![], vec![]);
        program.functions = vec![
            std::sync::Arc::new(base_retry),
            std::sync::Arc::new(main_anon),
        ];
        program.base_function_count = 1;
        program.main.stmts = vec![Stmt::Expr {
            expr: Expr::FunctionRef {
                name: "main#anon".to_string().into(),
                span: test_span(),
            },
            span: test_span(),
        }];

        assert!(program_main_contains_block_function_defs(&program));
        assert!(should_skip_base_cache_for_program(&program, 1));
    }

    #[test]
    fn test_base_function_ref_does_not_force_base_cache_skip_issue_8469() {
        let mut base_retry = empty_function("retry");
        base_retry.kwparams = vec![kwparam("check")];

        let mut program = minimal_program(vec![], vec![]);
        program.functions = vec![std::sync::Arc::new(base_retry)];
        program.base_function_count = 1;
        program.main.stmts = vec![Stmt::Expr {
            expr: Expr::FunctionRef {
                name: "retry".to_string().into(),
                span: test_span(),
            },
            span: test_span(),
        }];

        assert!(!program_main_contains_block_function_defs(&program));
        assert!(!should_skip_base_cache_for_program(&program, 1));
    }

    #[test]
    fn test_direct_unitrange_collect_uses_base_dispatch_4266() {
        let program = parse_and_lower_ok("result = collect(1:3)");
        let compiled =
            crate::compile::compile_core_program(&program).expect("compile collect(1:3)");
        let collect_code = instructions_after_last_make_range(&compiled.code);

        assert!(
            collect_code.iter().any(|instr| matches!(
                instr,
                Instr::Call(_, 1) | Instr::CallResolved(_, 1) | Instr::CallSpecialize(_, 1)
            )),
            "direct UnitRange collect should call the Base collect method, got {collect_code:?}"
        );
        assert!(
            !collect_code
                .iter()
                .any(|instr| matches!(instr, Instr::CallBuiltin(BuiltinId::RangeCollect, 1))),
            "direct UnitRange collect should not use the static RangeCollect boundary, got {collect_code:?}"
        );
    }

    #[test]
    fn test_direct_steprange_collect_uses_base_dispatch_4266() {
        let program = parse_and_lower_ok("result = collect(1:2:5)");
        let compiled =
            crate::compile::compile_core_program(&program).expect("compile collect(1:2:5)");
        let collect_code = instructions_after_last_make_range(&compiled.code);

        assert!(
            collect_code
                .iter()
                .any(|instr| matches!(
                    instr,
                    Instr::Call(_, 1) | Instr::CallResolved(_, 1) | Instr::CallSpecialize(_, 1)
                )),
            "direct integer StepRange collect should call the Base collect method, got {collect_code:?}"
        );
        assert!(
            !collect_code
                .iter()
                .any(|instr| matches!(instr, Instr::CallBuiltin(BuiltinId::RangeCollect, 1))),
            "direct integer StepRange collect should not use the static RangeCollect boundary, got {collect_code:?}"
        );
    }

    #[test]
    fn test_direct_float_range_collect_uses_base_dispatch_4266() {
        let program = parse_and_lower_ok("result = collect(1.0:0.5:2.0)");
        let compiled =
            crate::compile::compile_core_program(&program).expect("compile collect(1.0:0.5:2.0)");
        let collect_code = instructions_after_last_make_range(&compiled.code);

        assert!(
            collect_code
                .iter()
                .any(|instr| matches!(
                    instr,
                    Instr::Call(_, 1) | Instr::CallResolved(_, 1) | Instr::CallSpecialize(_, 1)
                )),
            "direct floating range collect should call the Base collect method, got {collect_code:?}"
        );
        assert!(
            !collect_code
                .iter()
                .any(|instr| matches!(instr, Instr::CallBuiltin(BuiltinId::RangeCollect, 1))),
            "direct floating range collect should not use the static RangeCollect boundary, got {collect_code:?}"
        );
    }

    #[test]
    fn test_runtime_any_steprange_collect_scores_base_candidate_4266() {
        let program = parse_and_lower_ok(
            r#"
collect_runtime_range_4266(x::Any) = collect(x)
result = collect_runtime_range_4266(1:2:7)
"#,
        );
        let compiled = crate::compile::compile_core_program(&program)
            .expect("compile runtime Any StepRange collect");

        assert!(
            compiled.code.iter().any(|instr| matches!(instr, Instr::CallDynamic(operands)
                if operands.arg_count == 1 && operands.candidates.iter().any(|c| matches!(c,
                    crate::bytecode::DynamicCallCandidate::Method(idx)
                        if compiled.functions.get(*idx)
                            .and_then(|f| f.param_julia_types.first())
                            .is_some_and(|ty| ty.to_string().starts_with("StepRange")))))),
            "runtime Any StepRange collect should score the Base StepRange collect candidate before the native range fallback, got {:?}",
            compiled.code
        );
    }

    #[test]
    fn test_direct_generator_collect_uses_dynamic_boundary_4265() {
        let program = parse_and_lower_ok(
            r#"
inc4265(x) = x + 1
result = collect(Base.Generator(inc4265, [1, 2, 3]))
"#,
        );
        let compiled = crate::compile::compile_core_program(&program)
            .expect("compile collect(Base.Generator(...))");
        let collect_code = instructions_after_last_make_generator(&compiled.code);

        assert!(
            collect_code
                .iter()
                .any(|instr| matches!(instr, Instr::CallDynamic(operands)
                    if operands.arg_count == 1 && operands.candidates.contains(&crate::bytecode::DynamicCallCandidate::NativeIterator(
                        crate::bytecode::NativeIteratorKind::Generator)))),
            "direct Generator collect should route through dynamic collect sentinel, got {collect_code:?}"
        );
        assert!(
            !collect_code
                .iter()
                .any(|instr| matches!(instr, Instr::CallBuiltin(BuiltinId::RangeCollect, 1))),
            "direct Generator collect should not use the static RangeCollect boundary, got {collect_code:?}"
        );
    }

    #[test]
    fn test_array_undef_constructor_routes_through_base_helper_4018() {
        let program = parse_and_lower_ok("result = Array{Int64}(undef, 2, 3)");
        let compiled = crate::compile::compile_core_program(&program).expect("compile Array undef");

        assert!(
            compiled
                .code
                .iter()
                .any(|instr| matches!(instr, Instr::PushFunction(name) if name == "_array_undef_from_dims")),
            "Array{{T}}(undef, dims...) should call the Pure Julia allocation helper, got {:?}",
            compiled.code
        );
        assert!(
            !compiled.code.iter().any(|instr| matches!(
                instr,
                Instr::AllocUndefTyped(..)
                    | Instr::AllocUndefTypedFromTuple(..)
                    | Instr::AllocUndefDynamicTyped(..)
                    | Instr::AllocUndefDynamicTypedFromTuple
            )),
            "Array{{T}}(undef, dims...) should not compile to AllocUndef* instructions, got {:?}",
            compiled.code
        );
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
                field_julia_types: vec![JuliaType::Float64, JuliaType::Float64],
                parent_type: None,
            },
            StructDefInfo {
                name: "Rational{Int64}".to_string(),
                is_mutable: false,
                fields: vec![
                    ("num".to_string(), ValueType::I64),
                    ("den".to_string(), ValueType::I64),
                ],
                field_julia_types: vec![JuliaType::Int64, JuliaType::Int64],
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
            field_julia_types: vec![JuliaType::Int64, JuliaType::Int64],
            parent_type: None,
        }];

        // Simulate: promote_rule(::Type{Rational{Int64}}, ::Type{Int64}) = Rational{Int64}
        // This mirrors how extract_promotion_rules processes MethodSig entries.
        let param1 = JuliaType::TypeOf(Box::new(JuliaType::Struct("Rational{Int64}".to_string())));
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
            definition_order: 0,
        };
        // Pattern: Stmt::Expr { expr: Var("Int64") } → "Int64"
        let body = Block {
            stmts: vec![Stmt::Expr {
                expr: Expr::Var("Int64".to_string().into(), span),
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
            definition_order: 0,
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

        assert!(
            is_cache_initialized(),
            "cache should be populated after compile"
        );
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

    /// Regression test: `clear_non_base_cache()` must preserve `BASE_CACHE` (the
    /// whole point is to let the fixture harness reuse it across fixtures within a
    /// chunk process) while still clearing `PROGRAM_CACHE` so per-fixture program
    /// results don't leak. The promotion registry is cleared too, but
    /// `get_or_init_base_cache` transparently replays it from the still-populated
    /// `BASE_CACHE` on the next `compile_with_cache()` call (Issue #3036), so this
    /// does not reintroduce the BASE_CACHE/registry desync from Issue #3038.
    #[test]
    fn test_clear_non_base_cache_preserves_base_cache() {
        clear_cache();

        let program = parse_and_lower_ok("zznbc = 1");
        let hash = compute_program_hash(
            &program,
            &std::collections::HashMap::new(),
            &std::collections::HashMap::new(),
        );
        compile_with_cache(&program).expect("first compile must succeed");
        compile_with_cache(&program).expect("second compile must succeed");

        assert!(is_cache_initialized(), "cache should be populated");
        assert!(
            PROGRAM_CACHE.with(|c| c.borrow().contains_key(&hash)),
            "second compile of the same program must store into PROGRAM_CACHE"
        );

        clear_non_base_cache();

        assert!(
            is_cache_initialized(),
            "clear_non_base_cache() must NOT clear BASE_CACHE"
        );
        assert!(
            !PROGRAM_CACHE.with(|c| c.borrow().contains_key(&hash)),
            "clear_non_base_cache() must clear PROGRAM_CACHE"
        );
        assert!(
            !promotion::is_registry_initialized(),
            "clear_non_base_cache() must clear the promotion registry"
        );

        // The registry desync (Issue #3038) is avoided because the next
        // compile transparently replays promotion rules from BASE_CACHE
        // (Issue #3036) instead of needing a full Base recompile.
        compile_with_cache(&program).expect("compile after clear_non_base_cache must succeed");
        assert!(
            promotion::is_registry_initialized(),
            "registry must be repopulated (via replay, not recompile) on next compile"
        );
        assert!(
            promotion::get_registry_size() > 0,
            "replayed registry must have rules"
        );

        clear_cache();
    }

    /// Regression test: `clear_program_cache()` is the narrow reset used by
    /// alternate-compiler-gate parity harnesses (Issue #9865). It must clear the
    /// full-program cache so a second gate recompiles, while preserving Base and
    /// Base-derived registries so the second pass does not pay Base replay cost.
    #[test]
    fn test_clear_program_cache_preserves_base_and_registry() {
        clear_cache();

        let program = parse_and_lower_ok("zzpc = 1");
        let hash = compute_program_hash(
            &program,
            &std::collections::HashMap::new(),
            &std::collections::HashMap::new(),
        );
        compile_with_cache(&program).expect("first compile must succeed");
        compile_with_cache(&program).expect("second compile must succeed");

        assert!(is_cache_initialized(), "Base cache should be populated");
        assert!(
            promotion::is_registry_initialized(),
            "promotion registry should be populated"
        );
        assert!(
            PROGRAM_CACHE.with(|c| c.borrow().contains_key(&hash)),
            "second compile of the same program must store into PROGRAM_CACHE"
        );

        clear_program_cache();

        assert!(
            is_cache_initialized(),
            "clear_program_cache() must NOT clear BASE_CACHE"
        );
        assert!(
            promotion::is_registry_initialized(),
            "clear_program_cache() must NOT clear the promotion registry"
        );
        assert!(
            !PROGRAM_CACHE.with(|c| c.borrow().contains_key(&hash)),
            "clear_program_cache() must clear PROGRAM_CACHE"
        );

        clear_cache();
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

        // Issue #5070: Union-typed promote_rule methods (e.g.
        // promote_rule(::Type{Int64}, ::Union{Type{Int16}, ...}) and the
        // Rational Union rules) must be flattened so every concrete pair is
        // registered, including the previously-missing UInt / Int128 partners.
        assert_eq!(
            promotion::promote_type("Int64", "Int16"),
            "Int64",
            "Union-typed integer promote_rule must register each concrete member (Issue #5070)"
        );
        assert_eq!(
            promotion::promote_type("Rational{Int8}", "UInt16"),
            "Rational{UInt16}",
            "Rational + unsigned integer must register via Union expansion (Issue #5070)"
        );
    }

    /// Issue #5093: Base compilation should retain an in-memory snapshot of
    /// inference return results so later Base-cache hits can seed their shared
    /// inference engine without re-inferring Base functions.
    #[test]
    fn test_inference_results_populated_after_base_compilation_5093() {
        clear_cache();

        let cached_base =
            compile_base_functions_from_source().expect("Base source compilation must succeed");
        assert!(
            !cached_base.inference_results.is_empty(),
            "Base source compilation should carry inference return results for in-memory replay"
        );

        clear_cache();
    }

    /// Issue #7357: persistent/embedded Base caches retain runtime
    /// specialization artifacts so WASM warm compilation can restore cached Base
    /// `CallSpecialize` metadata without rescanning every Base function.
    #[test]
    fn test_base_cache_persists_specializable_functions_7357() {
        use crate::compile::precompile::{deserialize_base_cache, serialize_base_cache};

        clear_cache();

        let cached_base =
            compile_base_functions_from_source().expect("Base source compilation must succeed");
        let original_specializable_count = cached_base.compiled.specializable_functions.len();
        assert!(
            original_specializable_count > 0,
            "Base compilation should retain runtime specialization targets"
        );
        let original_runtime_map_count = cached_base.compiled.runtime_specialization_map.len();
        assert!(
            original_runtime_map_count > 0,
            "Base compilation should retain runtime specialization mappings"
        );
        assert!(
            cached_base.compiled.compile_context.is_some(),
            "Source-compiled Base should carry a runtime compile context"
        );

        let bytes = serialize_base_cache(
            &cached_base.compiled,
            &cached_base.method_tables,
            &cached_base.closure_captures,
            &cached_base.inference_results,
        )
        .expect("serialization must succeed");
        let mut restored =
            deserialize_base_cache(&bytes).expect("deserialization must succeed with valid bytes");

        assert_eq!(
            restored.compiled.specializable_functions.len(),
            original_specializable_count,
            "persistent Base caches must retain specializable_functions for warm compile restore"
        );
        assert_eq!(
            restored.compiled.runtime_specialization_map.len(),
            original_runtime_map_count,
            "persistent Base caches must retain runtime specialization mappings"
        );
        assert!(
            restored.inference_results.is_empty(),
            "persistent Base caches intentionally omit inference snapshots (Issue #6348)"
        );
        assert!(
            restored.compiled.compile_context.is_none(),
            "compile_context is intentionally skipped in the serialized payload"
        );

        restored.inference_results = cached_base.inference_results.clone();
        assert!(
            !restored.inference_results.is_empty(),
            "test setup should simulate a legacy same-version cache with inference snapshots"
        );
        let restored_cached = cached_base_from_serialized(restored, "test");
        assert_eq!(
            restored_cached.compiled.specializable_functions.len(),
            original_specializable_count,
            "restored CachedBase should keep specialization IR for warm compile restore"
        );
        assert_eq!(
            restored_cached.compiled.runtime_specialization_map.len(),
            original_runtime_map_count,
            "restored CachedBase should keep runtime specialization mappings"
        );
        assert!(
            restored_cached.compiled.compile_context.is_some(),
            "persisted specialization IR should allow cached Base compile_context restoration"
        );
        assert!(
            restored_cached.inference_results.is_empty(),
            "serialized Base cache loads must ignore legacy inference snapshots"
        );

        clear_cache();
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
        let (compiled, method_tables, closure_captures, inference_results) =
            export_base_cache().expect("Base cache must be populated after compilation");

        // Serialize to bytes
        let bytes = serialize_base_cache(
            &compiled,
            &method_tables,
            &closure_captures,
            &inference_results,
        )
        .expect("serialization must succeed");
        assert!(!bytes.is_empty(), "Serialized cache must be non-empty");

        // Deserialize back
        let restored =
            deserialize_base_cache(&bytes).expect("deserialization must succeed with valid bytes");

        // Verify promotion rules are non-empty in the restored cache
        assert!(
            !restored.promotion_rules.is_empty(),
            "Deserialized cache must contain promotion rules. \
             Got 0 rules — serialize_base_cache is not capturing the registry. \
             See Issue #3025 and #3028."
        );
        assert!(
            restored.inference_results.is_empty(),
            "Persistent Base caches intentionally omit inference results; in-memory source cache hits still retain them"
        );

        // Must have many rules (Base defines ~168 concrete promote_rule methods)
        assert!(
            restored.promotion_rules.len() > 50,
            "Expected >50 promotion rules in deserialized cache, got {}",
            restored.promotion_rules.len()
        );

        // Verify specific rules are present
        let has_int64_float64 = restored.promotion_rules.iter().any(|(t1, t2, ret)| {
            t1 == "Int64" && t2 == "Float64" && ret == "Float64"
                || t1 == "Float64" && t2 == "Int64" && ret == "Float64"
        });
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

    /// Issue #6348 (phase 2): the warm-start prefetch hands the prelude
    /// function clones to at most one consumer and rejects length mismatches.
    #[cfg(not(target_arch = "wasm32"))]
    #[test]
    fn test_warm_start_prefetch_roundtrip_issue_6348() {
        let prelude_len = crate::get_prelude_program()
            .map(|p| p.functions.len())
            .expect("prelude program must load");

        // Nothing prefetched yet -> fallback path.
        assert!(take_prefetched_base_function_table(prelude_len).is_none());

        begin_warm_start_prefetch();
        let (function_table, ambiguous_functions) =
            take_prefetched_base_function_table(prelude_len)
                .expect("prefetched base function table must match the prelude length");
        // Issue #10114: the snapshot's `function_table`/`ambiguous_functions`
        // must match building it directly from the same (unrenamed) prelude
        // functions.
        let prelude = crate::get_prelude_program().expect("prelude program must load");
        let (expected_table, expected_ambiguous) =
            crate::compile::abstract_interp::engine::build_function_table(
                prelude.functions.iter().map(|f| (**f).clone()),
            );
        assert_eq!(function_table.len(), expected_table.len());
        assert_eq!(ambiguous_functions, expected_ambiguous);
        for (name, func) in &expected_table {
            let got = function_table
                .get(name)
                .unwrap_or_else(|| panic!("prefetched function_table missing `{name}`"));
            assert_eq!(
                got.params.len(),
                func.params.len(),
                "param count for {name}"
            );
        }

        // Consumed at most once.
        assert!(take_prefetched_base_function_table(prelude_len).is_none());

        // A length mismatch (e.g. Base-redefinition merge) must be rejected.
        begin_warm_start_prefetch();
        assert!(take_prefetched_base_function_table(prelude_len + 1).is_none());
    }

    /// Issue #6348: PROGRAM_CACHE must not pay the CompiledProgram deep-clone
    /// on a program's first compile (one-shot CLI runs). The program is stored
    /// on the second compile of the same hash, and full hits begin on the third.
    #[test]
    fn test_program_cache_stores_only_on_second_compile_issue_6348() {
        clear_cache();

        let program = parse_and_lower_ok("zz6348 = 41");
        let hash = compute_program_hash(
            &program,
            &std::collections::HashMap::new(),
            &std::collections::HashMap::new(),
        );

        compile_with_cache(&program).expect("first compile must succeed");
        assert!(
            !PROGRAM_CACHE.with(|c| c.borrow().contains_key(&hash)),
            "first compile must not store into PROGRAM_CACHE (one-shot runs skip the deep clone)"
        );

        compile_with_cache(&program).expect("second compile must succeed");
        assert!(
            PROGRAM_CACHE.with(|c| c.borrow().contains_key(&hash)),
            "second compile of the same program must store into PROGRAM_CACHE"
        );

        clear_cache();
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

    /// Characterization/regression test for Issue #10113: a cached-Base
    /// compile must restore a Base function's `MethodTable` by sharing the
    /// cache's own `Arc<Vec<MethodSig>>` (via `MethodTable::clone_for_reprojection`
    /// in `seed_outputs_from_cache`), never by rebuilding it through
    /// `MethodTable::add_method` (which `Arc::make_mut`s a private copy on
    /// first use). A regression that reintroduced a full per-function
    /// `add_method` rebuild for cached Base functions would break this
    /// `Arc::ptr_eq` — the two `Arc`s would no longer point at the same
    /// allocation.
    #[test]
    fn cached_base_method_table_reuses_shared_arc_10113() {
        let program = parse_and_lower_ok("println(\"Hello World\")");
        let merged = if program.base_function_count > 0 {
            program.clone()
        } else {
            crate::compile::base_merge::merge_with_precompiled_base(&program).program
        };
        let bundle = compile_core_bundle_with_base_cache(
            &program,
            &merged,
            &HashMap::new(),
            &HashMap::new(),
            None,
            None,
            None,
            None,
        )
        .expect("cached-Base compile must succeed");
        let base = get_or_init_base_cache().expect("base cache must initialize");
        // `length` is a genuine Base function (never touched by this trivial
        // program's user/prelude registration), so its table must stay
        // Arc-shared end to end.
        let base_table = base
            .method_tables
            .get("length")
            .expect("base cache must have a `length` method table");
        let bundle_table = bundle
            .method_tables
            .get("length")
            .expect("compiled output must have a `length` method table");
        assert!(
            std::sync::Arc::ptr_eq(&base_table.methods, &bundle_table.methods),
            "cached-Base compile rebuilt the `length` method table instead of \
             reusing the cache's shared Arc (Issue #10113 regression)"
        );
    }
    // ─── Cache-restore parity guards (Issue #10265, #10092 class) ───────────
    //
    // Invariant: a compile context rebuilt on a cache/serialization boundary
    // must reproduce EXACTLY what the fresh compile pipeline builds. Defaulted
    // reconstruction (`false`, `HashMap::new()`, …) of a field the fresh path
    // populates is forbidden without an Issue-tracked justification — that is
    // how #10092 happened (`has_inner_constructor: false` on both Base-cache
    // restore paths made `WeakRef(x)` bypass its outer constructor, so weak
    // cells were never registered with the GC).
    //
    // The exhaustive destructuring in `assert_compile_context_parity` is the
    // compile-error-shaped half of the guard (precedent: #10060): adding a
    // field to `RuntimeCompileContext` or `StructInfo` fails compilation HERE
    // until the author decides how the restore paths reproduce it (and either
    // adds a parity assertion or a documented, Issue-tracked exemption below).
    // See docs/vm/CACHE_ARCHITECTURE.md "Cache-restore parity invariant".

    /// Representative corpus exercising every struct-table dimension the
    /// restore paths must reproduce: inner-constructor suppression (#10092),
    /// mutability, plain fields, parametric definitions + instantiations,
    /// module-qualified structs, type aliases, primitive types, and a user
    /// specialization-safety overrides, including module-owned methods whose
    /// array receiver is an alias (Issue #10334).
    const PARITY_CORPUS_10265: &str = r#"
mutable struct MutPlain10265
    x::Int64
end

struct InnerCtor10265
    x::Int64
    InnerCtor10265(x::Int64) = new(x + 1)
end

struct Box10265{T}
    v::T
end

module ParityMod10265
export ModBox10265
abstract type ModAbstract10265 end
struct ModInnerCtor10265
    v::Int64
    ModInnerCtor10265(v::Int64) = new(v)
end

using .ParityMod10265
struct ModBox10265{T} <: ModAbstract10265
    v::T
end

const AliasVector10334 = Vector{Int64}
struct PropertyTarget10334
    value::Int64
end

Base.getindex(v::AliasVector10334, i::Int64) = v
Base.setindex!(v::AliasVector10334, x::Int64, i::Int64) = v
Base.getproperty(x::PropertyTarget10334, name::Symbol) = x
end

baremodule BareParityMod10265 end
baremodule ExplicitBaseParityMod10265
using Base
end
module ParentParityMod10265
baremodule NestedBareParityMod10265 end
end

const IntAlias10265 = Int64
primitive type Prim10265 8 end

function touch10265()
    b = Box10265{Int64}(1)
    m = ParityMod10265.ModInnerCtor10265(2)
    mb = ModBox10265{Float64}(1.5)
    i = InnerCtor10265(0)
    mp = MutPlain10265(3)
    w = WeakRef(mp)
    b.v + m.v + i.x + mp.x + Int64(mb.v - 1.5) + [10, 20][1]
end
println(touch10265())
"#;

    /// Flatten a struct table into a sorted, exhaustively-destructured view.
    /// `StructInfo` is destructured WITHOUT `..` on purpose: a new field
    /// added to `StructInfo` must fail compilation here so its restore-path
    /// story gets decided explicitly (Issue #10265).
    fn struct_table_snapshot(
        table: &crate::compile::StructRegistry,
    ) -> Vec<(String, usize, bool, Vec<(String, ValueType)>, bool)> {
        let mut rows: Vec<_> = table
            .iter()
            .map(|(name, info)| {
                let crate::compile::StructInfo {
                    type_id,
                    is_mutable,
                    fields,
                    has_inner_constructor,
                } = info;
                (
                    name.clone(),
                    *type_id,
                    *is_mutable,
                    fields.clone(),
                    *has_inner_constructor,
                )
            })
            .collect();
        rows.sort_by(|a, b| a.0.cmp(&b.0));
        rows
    }

    /// Every registered name projected to the OWNER-SCOPED IDENTITY behind it
    /// (Issue #11078): the owning module path and the concrete-type index of
    /// the `StructId` the name resolves to.
    ///
    /// This is the fresh-vs-cache-restore identity parity the #11078
    /// acceptance criteria ask for, and it is strictly stronger than
    /// `struct_table_snapshot`: that one compares the LAYOUT reachable under
    /// each name, while this one compares WHICH DECLARATION each name denotes.
    /// The two lanes build the registry in genuinely different orders (the
    /// cached lane seeds every cached `struct_defs` entry, parametric
    /// instantiations included, up front; the fresh lane creates those much
    /// later, after the user's structs), so an id minted from a registration
    /// counter would diverge here — this is what pins the "derive, don't
    /// persist" (Pattern A) allocation to being genuinely lane-invariant.
    fn struct_id_snapshot(
        table: &crate::compile::StructRegistry,
    ) -> Vec<(String, Option<String>, usize)> {
        let mut rows: Vec<_> = table
            .keys()
            .filter_map(|name| {
                let id = table.id_of(name)?;
                Some((
                    name.clone(),
                    table.owner_path(id).map(str::to_string),
                    id.type_id(),
                ))
            })
            .collect();
        rows.sort();
        rows
    }

    fn sorted_debug_map<V: std::fmt::Debug>(map: &HashMap<String, V>) -> Vec<(String, String)> {
        let mut rows: Vec<_> = map
            .iter()
            .map(|(k, v)| (k.clone(), format!("{v:?}")))
            .collect();
        rows.sort();
        rows
    }

    /// Field-by-field parity between a fresh-compile `RuntimeCompileContext`
    /// and one rebuilt by `restore_compile_context_from_program` after the
    /// serialization boundary dropped it (`compile_context` is
    /// `#[serde(skip)]`).
    fn assert_compile_context_parity(
        fresh: &RuntimeCompileContext,
        restored: &RuntimeCompileContext,
    ) {
        // Exhaustive destructuring: adding a field to `RuntimeCompileContext`
        // must fail compilation here (Issue #10265 guard). Decide how the
        // restore paths reproduce the new field BEFORE re-listing it.
        let RuntimeCompileContext {
            struct_table: fresh_struct_table,
            struct_defs: fresh_struct_defs,
            parametric_structs: fresh_parametric_structs,
            base_parametric_structs: fresh_base_parametric_structs,
            type_aliases: fresh_type_aliases,
            module_imported_bindings: fresh_module_imported_bindings,
            module_base_exports_visibility: fresh_module_base_exports_visibility,
            module_implicit_standard_bindings: fresh_module_implicit_standard_bindings,
            base_exported_names: fresh_base_exported_names,
            inference_global_types: fresh_inference_global_types,
            primitive_types: fresh_primitive_types,
            disable_array_getindex_specialization: fresh_disable_getindex,
            disable_array_setindex_specialization: fresh_disable_setindex,
            disable_field_access_specialization: fresh_disable_field_access,
            module_registry: fresh_module_registry,
        } = fresh;
        let RuntimeCompileContext {
            struct_table: restored_struct_table,
            struct_defs: restored_struct_defs,
            parametric_structs: restored_parametric_structs,
            base_parametric_structs: restored_base_parametric_structs,
            type_aliases: restored_type_aliases,
            module_imported_bindings: restored_module_imported_bindings,
            module_base_exports_visibility: restored_module_base_exports_visibility,
            module_implicit_standard_bindings: restored_module_implicit_standard_bindings,
            base_exported_names: restored_base_exported_names,
            inference_global_types: restored_inference_global_types,
            primitive_types: restored_primitive_types,
            disable_array_getindex_specialization: restored_disable_getindex,
            disable_array_setindex_specialization: restored_disable_setindex,
            disable_field_access_specialization: restored_disable_field_access,
            module_registry: restored_module_registry,
        } = restored;

        assert_eq!(
            struct_table_snapshot(fresh_struct_table),
            struct_table_snapshot(restored_struct_table),
            "struct_table must be identical after cache restore (the #10092 \
             field: has_inner_constructor)"
        );
        assert_eq!(
            struct_id_snapshot(fresh_struct_table),
            struct_id_snapshot(restored_struct_table),
            "every name must resolve to the SAME owner-scoped StructId after a \
             cache restore as it does on a fresh compile — the ids are DERIVED \
             on both lanes, never persisted or relocated (Issue #11078, \
             docs/vm/CACHE_ARCHITECTURE.md Pattern A)"
        );
        assert_eq!(
            fresh_struct_defs
                .iter()
                .map(|d| format!("{d:?}"))
                .collect::<Vec<_>>(),
            restored_struct_defs
                .iter()
                .map(|d| format!("{d:?}"))
                .collect::<Vec<_>>(),
            "struct_defs must be carried through serialization unchanged"
        );
        assert_eq!(
            sorted_debug_map(fresh_parametric_structs),
            sorted_debug_map(restored_parametric_structs),
            "parametric_structs must be rebuilt identically from the IR"
        );
        assert_eq!(
            sorted_debug_map(fresh_base_parametric_structs),
            sorted_debug_map(restored_base_parametric_structs),
            "Base parametric structs must be rebuilt identically from the IR"
        );
        assert_eq!(
            fresh_type_aliases, restored_type_aliases,
            "type_aliases must be rebuilt identically from the IR"
        );
        assert_eq!(
            fresh_module_imported_bindings, restored_module_imported_bindings,
            "live imported bindings must be rebuilt identically from the IR"
        );
        assert_eq!(
            fresh_module_base_exports_visibility, restored_module_base_exports_visibility,
            "module Base-export visibility must be rebuilt identically from the IR"
        );
        assert_eq!(
            fresh_module_implicit_standard_bindings, restored_module_implicit_standard_bindings,
            "implicit module eval/include bindings must be rebuilt identically from the IR"
        );
        assert_eq!(
            fresh_base_exported_names, restored_base_exported_names,
            "the canonical Base export set must be identical after cache restore"
        );
        assert_eq!(
            fresh_inference_global_types, restored_inference_global_types,
            "inference_global_types must be persisted and restored exactly (Issue #10333)"
        );
        assert_eq!(
            fresh_primitive_types
                .iter()
                .map(|p| format!("{p:?}"))
                .collect::<Vec<_>>(),
            restored_primitive_types
                .iter()
                .map(|p| format!("{p:?}"))
                .collect::<Vec<_>>(),
            "primitive_types must be carried through serialization unchanged"
        );
        assert_eq!(
            fresh_disable_getindex, restored_disable_getindex,
            "disable_array_getindex_specialization must be persisted and restored exactly \
             (Issue #10334)"
        );
        assert_eq!(
            fresh_disable_setindex, restored_disable_setindex,
            "disable_array_setindex_specialization must be persisted and restored exactly \
             (Issue #10334)"
        );
        assert_eq!(
            fresh_disable_field_access, restored_disable_field_access,
            "disable_field_access_specialization must be persisted and restored exactly \
             (Issue #10334)"
        );
        // Issue #10988 Phase 2a: the restore path (`restore_compile_context_from_program`)
        // rebuilds `module_registry` by re-walking this program's module tree in
        // the same deterministic order the fresh-compile path uses
        // (`register_module_ids`), so every path must resolve to the SAME
        // `ModuleId` on both sides — the cache-relocation parity the epic's
        // acceptance criteria require ("fresh compile AND cache restore").
        for path in fresh_module_registry.paths() {
            assert_eq!(
                fresh_module_registry.lookup(path),
                restored_module_registry.lookup(path),
                "module {path:?} must resolve to the same ModuleId fresh and restored"
            );
        }
        assert_eq!(
            fresh_module_registry.len(),
            restored_module_registry.len(),
            "fresh and restored module_registry must register the same module set"
        );
    }

    /// Issue #10265 acceptance: fresh-compile vs cache-restored compile
    /// context parity across a serialization boundary — the generalized
    /// #10092 guard. The boundary is exercised with the REAL cache
    /// serializer (`precompile::cache_serialize`), which drops
    /// `compile_context` (`#[serde(skip)]`), then rebuilt with the REAL
    /// restore entry point (`restore_compile_context_from_program`, shared by
    /// the Base-cache restore and the `.sjvmbc` load path).
    #[test]
    fn restored_compile_context_matches_fresh_compile_10265() {
        let program = parse_and_lower_ok(PARITY_CORPUS_10265);
        let fresh = crate::compile::compile_core_program(&program).expect("fresh compile");
        let fresh_ctx = fresh
            .compile_context
            .as_ref()
            .expect("corpus must trigger a compile context (parametric structs, aliases)");

        let bytes = super::super::precompile::cache_serialize(&fresh).expect("cache serialize");
        let mut restored: CompiledProgram =
            super::super::precompile::cache_deserialize(&bytes).expect("cache deserialize");
        assert_eq!(
            fresh.specialization_disable_flags,
            crate::bytecode::SpecializationDisableFlags {
                array_getindex: true,
                array_setindex: true,
                field_access: true,
            },
            "fresh method tables must finalize all three module/alias safety decisions"
        );
        assert_eq!(
            restored.specialization_disable_flags,
            fresh.specialization_disable_flags,
            "serialization must preserve specialization policy before context restore (Issue #10334)"
        );
        assert!(
            restored.compile_context.is_none(),
            "compile_context must be #[serde(skip)]; if this changed, the \
             restore paths (and this guard) need a redesign"
        );
        restore_compile_context_from_program(&mut restored, &program);
        let restored_ctx = restored
            .compile_context
            .as_ref()
            .expect("restore must rebuild the compile context");

        let flags = |ctx: &RuntimeCompileContext| {
            (
                ctx.disable_array_getindex_specialization,
                ctx.disable_array_setindex_specialization,
                ctx.disable_field_access_specialization,
            )
        };
        assert_eq!(
            flags(fresh_ctx),
            (true, true, true),
            "module-owned alias-receiver overrides must activate all three fresh flags"
        );
        assert_eq!(
            flags(restored_ctx),
            flags(fresh_ctx),
            "cache restore must preserve module/alias specialization policy (Issue #10334)"
        );

        assert_compile_context_parity(fresh_ctx, restored_ctx);
    }

    /// Issue #10335: a seeded `PROGRAM_CACHE` hit decodes a postcard blob, so
    /// its `compile_context` is `None` (`#[serde(skip)]`) unless the lookup
    /// restores it. The embedded entry list comes from a compile-time
    /// `include_bytes!` a unit test cannot swap out, so this injects the
    /// serialized entry directly into `SEEDED_PROGRAM_CACHE_RAW` and drives the
    /// REAL `seeded_program_cache_lookup`, asserting the hit's context matches
    /// a fresh compile of the identical source — the same parity the Base-cache
    /// and `.sjvmbc` lanes already guarantee.
    #[test]
    fn seeded_program_cache_hit_restores_compile_context_10335() {
        let program = parse_and_lower_ok(PARITY_CORPUS_10265);
        let fresh = crate::compile::compile_core_program(&program).expect("fresh compile");
        let fresh_ctx = fresh
            .compile_context
            .as_ref()
            .expect("corpus must trigger a compile context (parametric structs, aliases)");

        let hash = compute_program_hash(&program, &HashMap::new(), &HashMap::new());
        let bytes = super::super::precompile::cache_serialize(&fresh).expect("cache serialize");
        SEEDED_PROGRAM_CACHE_RAW.with(|slot| {
            *slot.borrow_mut() = Some(vec![(hash, bytes)]);
        });

        let hit = seeded_program_cache_lookup(hash, &program);

        // Undo the injection before asserting, so a failure below cannot leave
        // this thread's seeded list (or the PROGRAM_CACHE insertion the lookup
        // performs) visible to other tests.
        SEEDED_PROGRAM_CACHE_RAW.with(|slot| {
            *slot.borrow_mut() = None;
        });
        PROGRAM_CACHE.with(|cache| {
            cache.borrow_mut().remove(&hash);
        });

        let hit = hit.expect("injected entry must be a seeded hit");
        let hit_ctx = hit
            .compile_context
            .as_ref()
            .expect("a seeded hit must carry a restored compile context (Issue #10335)");
        assert_compile_context_parity(fresh_ctx, hit_ctx);
    }

    const SAME_NAME_DIFFERENT_MODULE_CORPUS_10988: &str = r#"
struct Holder10988{T}
    v::T
end

module A10988
module Sub10988
x = 1
end
end

module B10988
module Sub10988
y = 2
end
end

function touch10988()
    h = Holder10988{Int64}(1)
    h.v
end
println(touch10988())
"#;

    /// Issue #10988 Phase 2a acceptance: two sibling submodules sharing the
    /// same LOCAL name under different parents (`A10988.Sub10988` /
    /// `B10988.Sub10988`) must resolve to distinct `ModuleId`s, and a
    /// cache-restored session must recover the SAME ids a fresh compile of
    /// the identical source assigns — both halves of the epic's own
    /// acceptance criterion ("Same-name-different-module regression tests
    /// (fresh compile AND cache restore)").
    #[test]
    fn same_name_different_module_gets_distinct_and_stable_ids_issue_10988() {
        let program = parse_and_lower_ok(SAME_NAME_DIFFERENT_MODULE_CORPUS_10988);
        let fresh = crate::compile::compile_core_program(&program).expect("fresh compile");
        let fresh_ctx = fresh
            .compile_context
            .as_ref()
            .expect("corpus must trigger a compile context (top-level parametric struct)");

        let a_sub = fresh_ctx.module_registry.lookup("A10988.Sub10988");
        let b_sub = fresh_ctx.module_registry.lookup("B10988.Sub10988");
        assert!(a_sub.is_some(), "A10988.Sub10988 must be registered");
        assert!(b_sub.is_some(), "B10988.Sub10988 must be registered");
        assert_ne!(
            a_sub, b_sub,
            "same-named submodules of different parents must get distinct ModuleIds"
        );

        let bytes = super::super::precompile::cache_serialize(&fresh).expect("cache serialize");
        let mut restored: CompiledProgram =
            super::super::precompile::cache_deserialize(&bytes).expect("cache deserialize");
        restore_compile_context_from_program(&mut restored, &program);
        let restored_ctx = restored
            .compile_context
            .as_ref()
            .expect("restore must rebuild the compile context");

        assert_eq!(
            restored_ctx.module_registry.lookup("A10988.Sub10988"),
            a_sub,
            "cache restore must recover the same ModuleId fresh compile assigned"
        );
        assert_eq!(
            restored_ctx.module_registry.lookup("B10988.Sub10988"),
            b_sub,
            "cache restore must recover the same ModuleId fresh compile assigned"
        );
    }

    const SNAPSHOT_CORPUS_10462: &str = r#"
const SnapshotGlobal10462 = 41

struct SnapshotBox10462{T}
    value::T
end

module SnapshotMod10462
struct Index10462{T}
    value::T
end
Base.getindex(values::Vector{Int64}, index::Index10462{Int64}) = values[index.value]
end

snapshot_method_10462(x::SnapshotBox10462{T}) where {T} = x.value
snapshot_result_10462 = snapshot_method_10462(SnapshotBox10462(SnapshotGlobal10462))
"#;

    fn snapshot_output_10462(program: &Program) -> super::super::pipeline_ctx::CoreCompileOutput {
        super::super::pipeline_ctx::compile_core_program_internal(
            program,
            &HashMap::new(),
            &HashMap::new(),
            super::super::CompilerCacheInput::default(),
        )
        .expect("snapshot corpus compile")
    }

    fn seed_snapshot_promotion_registry_10462() {
        // The promotion registry is thread-local external compile state, not a
        // CompiledProgram field, and direct compile_core_program_internal does
        // not populate it. Seed one deterministic semantic rule so the harness
        // can prove manual same-process restore retains the registry while a
        // fresh-process .sjvmbc load loses it (Issue #10339).
        promotion::register_promotion_rule(
            "SnapshotBox10462{Int64}",
            "Int64",
            "SnapshotBox10462{Int64}",
        );
        promotion::register_promotion_rule(
            "Int64",
            "SnapshotBox10462{Int64}",
            "SnapshotBox10462{Int64}",
        );
        promotion::mark_registry_initialized();
    }

    struct PromotionRegistryGuard10462;

    impl PromotionRegistryGuard10462 {
        fn capture_after_clear_cache() -> Self {
            assert!(BASE_CACHE.with(|cache| cache.borrow().is_none()));
            assert!(promotion::get_all_promotion_rules().is_empty());
            assert!(!promotion::is_registry_initialized());
            Self
        }
    }

    impl Drop for PromotionRegistryGuard10462 {
        fn drop(&mut self) {
            // BASE_CACHE and the promotion registry are one coupled state
            // invariant (#3036/#3038); restore both to the clear baseline even
            // if a future test body initializes the Base cache before panicking.
            clear_cache();
        }
    }

    fn assert_tracked_snapshot_scoreboard_10462(
        scoreboard: &super::super::context_snapshot::CompileContextScoreboard,
        expected: &[(
            super::super::context_snapshot::CompileContextField,
            Option<u64>,
        )],
    ) {
        let actual: Vec<_> = scoreboard
            .mismatches()
            .iter()
            .map(|mismatch| (mismatch.field, mismatch.tracking_issue))
            .collect();
        assert_eq!(actual, expected, "{}", scoreboard.render());
    }

    #[test]
    fn compile_context_snapshot_is_deterministic_10462() {
        use super::super::context_snapshot::CompileContextSnapshot;

        clear_cache();
        let _promotion_guard = PromotionRegistryGuard10462::capture_after_clear_cache();
        let program = parse_and_lower_ok(SNAPSHOT_CORPUS_10462);
        let output = snapshot_output_10462(&program);
        seed_snapshot_promotion_registry_10462();
        let original_rules = promotion::get_all_promotion_rules();

        let first = CompileContextSnapshot::capture(&output.compiled);
        promotion::clear_registry();
        for (lhs, rhs, result) in original_rules.iter().rev() {
            promotion::register_promotion_rule(lhs, rhs, result);
        }
        promotion::mark_registry_initialized();
        let second = CompileContextSnapshot::capture(&output.compiled);

        promotion::clear_registry();
        for (lhs, rhs, result) in original_rules {
            promotion::register_promotion_rule(&lhs, &rhs, &result);
        }
        promotion::mark_registry_initialized();

        assert_eq!(
            first, second,
            "snapshot must ignore HashMap insertion order"
        );
        assert!(!first.semantic_structs.is_empty());
        assert!(!first.method_signatures.is_empty());
        assert!(!first.promotion_registry.rules.is_empty());
    }

    #[test]
    fn compile_context_snapshot_restore_lane_scoreboard_10462() {
        use super::super::context_snapshot::{
            CompileContextField as Field, CompileContextFieldValue, CompileContextScoreboard,
            CompileContextSnapshot, PromotionRegistrySnapshot,
        };

        clear_cache();
        let _promotion_guard = PromotionRegistryGuard10462::capture_after_clear_cache();
        let program = parse_and_lower_ok(SNAPSHOT_CORPUS_10462);
        let output = snapshot_output_10462(&program);
        seed_snapshot_promotion_registry_10462();
        let fresh = CompileContextSnapshot::capture(&output.compiled);

        let bytes = super::super::precompile::cache_serialize(&output.compiled)
            .expect("manual cache serialize");
        let mut manual: CompiledProgram =
            super::super::precompile::cache_deserialize(&bytes).expect("manual cache deserialize");
        restore_compile_context_from_program(&mut manual, &program);
        // A serialization boundary cannot inherit the source compiler's TLS.
        // Clearing here prevents same-process state from masquerading as
        // restoration evidence for the manual lane (Issue #10339).
        promotion::clear_registry();
        let manual = CompileContextSnapshot::capture(&manual);
        let manual_scoreboard = CompileContextScoreboard::compare(
            "manual serialize/restore",
            &fresh,
            &manual,
            |field| match field {
                // main_scope_names is #[serde(skip)] with a REPL-only
                // consumer (Issue #9182); the manual serialize/restore lane
                // has no promotion replay of its own — the .sjvmbc file lane
                // gained one in Issue #10339 (format v7), the remaining
                // manual-lane gap stays tracked under the #10462 epic.
                Field::MainScopeNames => Some(9182),
                Field::PromotionRegistry => Some(10462),
                _ => None,
            },
        );
        // The first Phase-0 RED run proved both struct fields match in this
        // module-parametric corpus; do not keep #10337 as a blanket exemption.
        assert_tracked_snapshot_scoreboard_10462(
            &manual_scoreboard,
            &[
                (Field::MainScopeNames, Some(9182)),
                (Field::PromotionRegistry, Some(10462)),
            ],
        );

        let expected_globals = vec![
            ("SnapshotGlobal10462".to_string(), ValueType::I64),
            ("snapshot_result_10462".to_string(), ValueType::Any),
        ];
        // The full typed snapshot retains Base/prelude bindings. Pin the
        // stable corpus delta rather than making unrelated Base additions
        // rewrite this test's expected value.
        let fresh_corpus_globals: Vec<_> = fresh
            .inference_global_types
            .iter()
            .filter(|(name, _)| name.contains("10462"))
            .cloned()
            .collect();
        assert_eq!(fresh_corpus_globals, expected_globals);
        assert_eq!(manual.inference_global_types, fresh.inference_global_types);
        assert_eq!(
            fresh.main_scope_names,
            vec![
                "SnapshotGlobal10462".to_string(),
                "snapshot_result_10462".to_string(),
            ]
        );
        assert_eq!(manual.main_scope_names, Vec::<String>::new());
        let expected_promotion = PromotionRegistrySnapshot {
            initialized: true,
            rules: vec![
                (
                    "Int64".to_string(),
                    "SnapshotBox10462{Int64}".to_string(),
                    "SnapshotBox10462{Int64}".to_string(),
                ),
                (
                    "SnapshotBox10462{Int64}".to_string(),
                    "Int64".to_string(),
                    "SnapshotBox10462{Int64}".to_string(),
                ),
            ],
        };
        assert_eq!(fresh.promotion_registry, expected_promotion);
        assert_eq!(
            manual.promotion_registry,
            PromotionRegistrySnapshot::default()
        );
        assert!(
            fresh.specialization_policy.disable_array_getindex,
            "module-owned getindex override must disable fresh specialization"
        );
        assert_eq!(
            manual.specialization_policy, fresh.specialization_policy,
            "manual restore must replay finalized specialization policy (Issue #10334)"
        );

        for mismatch in manual_scoreboard.mismatches() {
            match (&mismatch.field, &mismatch.fresh, &mismatch.restored) {
                (
                    Field::MainScopeNames,
                    CompileContextFieldValue::MainScopeNames(fresh_value),
                    CompileContextFieldValue::MainScopeNames(restored_value),
                ) => {
                    assert_eq!(fresh_value, &fresh.main_scope_names);
                    assert_eq!(restored_value, &manual.main_scope_names);
                }
                (
                    Field::PromotionRegistry,
                    CompileContextFieldValue::PromotionRegistry(fresh_value),
                    CompileContextFieldValue::PromotionRegistry(restored_value),
                ) => {
                    assert_eq!(fresh_value, &fresh.promotion_registry);
                    assert_eq!(restored_value, &manual.promotion_registry);
                }
                _ => panic!("unexpected or ill-typed manual mismatch: {mismatch:?}"),
            }
        }

        let mut path = std::env::temp_dir();
        path.push(format!(
            "sjulia_compile_context_snapshot_10462_{}.sjvmbc",
            std::process::id()
        ));
        // A real `.sjvmbc` producer saves from the process that just compiled
        // the program, i.e. with the promotion registry POPULATED — the v7
        // payload records those rules (Issue #10339). Re-seed (the manual lane
        // above cleared the TLS registry) so save captures what a real
        // compiling process would.
        seed_snapshot_promotion_registry_10462();
        crate::vm_bytecode_file::save(&program, &output.compiled, &path).expect("save .sjvmbc");
        // A real .sjvmbc consumer is a fresh process. Clearing the thread-local
        // registry here proves the rules observed after load came from the
        // payload replay, not from same-process leftovers (#10339).
        promotion::clear_registry();
        let loaded = crate::vm_bytecode_file::load(&path).expect("load .sjvmbc");
        let _ = std::fs::remove_file(&path);
        let sjvmbc = CompileContextSnapshot::capture(&loaded);
        let sjvmbc_scoreboard =
            CompileContextScoreboard::compare(".sjvmbc", &fresh, &sjvmbc, |field| match field {
                Field::MainScopeNames => Some(9182),
                _ => None,
            });
        assert_tracked_snapshot_scoreboard_10462(
            &sjvmbc_scoreboard,
            &[(Field::MainScopeNames, Some(9182))],
        );
        assert_eq!(sjvmbc.inference_global_types, fresh.inference_global_types);
        assert_eq!(sjvmbc.inference_global_types, manual.inference_global_types);
        assert_eq!(sjvmbc.main_scope_names, manual.main_scope_names);
        // Format v7 replays the save-time promotion rules and marks the
        // registry initialized on load (Issue #10339): full parity with fresh.
        assert_eq!(sjvmbc.promotion_registry, fresh.promotion_registry);
        assert_eq!(
            sjvmbc.specialization_policy, fresh.specialization_policy,
            ".sjvmbc restore must replay finalized specialization policy (Issue #10334)"
        );
    }

    #[test]
    fn promotion_registry_guard_restores_clear_cache_invariant_after_panic_10462() {
        clear_cache();
        let result = std::panic::catch_unwind(|| {
            let _guard = PromotionRegistryGuard10462::capture_after_clear_cache();
            get_or_init_base_cache().expect("panic test must initialize coupled Base state");
            assert!(BASE_CACHE.with(|cache| cache.borrow().is_some()));
            seed_snapshot_promotion_registry_10462();
            assert!(promotion::is_registry_initialized());
            panic!("exercise cleanup");
        });
        assert!(result.is_err());
        assert!(promotion::get_all_promotion_rules().is_empty());
        assert!(!promotion::is_registry_initialized());
        assert!(BASE_CACHE.with(|cache| cache.borrow().is_none()));
    }

    /// ONE id space, not two (Issue #11078). `StructRegistry` interns its
    /// owner module paths in its own `ModuleInternTable`, seeded by the same
    /// `register_module_ids` walk the pipeline uses for
    /// `CorePipeline::module_registry` / `RuntimeCompileContext::module_registry`.
    /// That the two agree is the justification for the registry minting owner
    /// ids at all — asserted here rather than left "true by construction",
    /// because a future edit that seeds the registry from a different source
    /// (or forgets to seed it) would silently create a SECOND `ModuleId` space
    /// for the same modules — exactly the smell #10988 avoided.
    #[test]
    fn struct_registry_owner_ids_agree_with_pipeline_module_registry_11078() {
        let program = parse_and_lower_ok(PARITY_CORPUS_10265);
        let compiled = crate::compile::compile_core_program(&program).expect("fresh compile");
        let ctx = compiled.compile_context.as_ref().expect("compile context");

        let struct_modules = ctx.struct_table.module_registry();
        for path in ctx.module_registry.paths() {
            assert_eq!(
                struct_modules.lookup(path),
                ctx.module_registry.lookup(path),
                "StructRegistry and the pipeline module registry must allocate \
                 the SAME ModuleId for module {path:?} (one id space, Issue #11078)"
            );
        }

        // And the ids are actually used: a module struct is owned by its module,
        // not by Main.
        let (mod_box_id, _) = ctx
            .struct_table
            .resolve("ParityMod10265.ModInnerCtor10265")
            .expect("module struct registered under its qualified name");
        assert_eq!(
            ctx.struct_table.owner_path(mod_box_id),
            Some("ParityMod10265")
        );
        assert_eq!(
            ctx.module_registry.lookup("ParityMod10265"),
            Some(mod_box_id.module()),
            "the struct's owner ModuleId must be the program's ModuleId for that module"
        );
    }

    /// The precondition that makes `StructRegistry::insert`'s lazy owner
    /// interning unreachable in production (Issue #11078): every struct name in
    /// a real compile is owned by a module that the program's module-tree walk
    /// already interned (or by `Main`). If this ever fails, some struct is
    /// registered under a qualified name whose owner is NOT a program module,
    /// `insert` would intern it lazily in struct-registration ORDER, and the
    /// fresh and cached lanes could then disagree on its `StructId` — the
    /// nondeterminism the seeded module table exists to prevent.
    #[test]
    fn struct_registry_owner_paths_are_all_program_modules_11078() {
        let program = parse_and_lower_ok(PARITY_CORPUS_10265);
        let compiled = crate::compile::compile_core_program(&program).expect("fresh compile");
        let ctx = compiled.compile_context.as_ref().expect("compile context");

        let known: std::collections::HashSet<&str> = ctx.module_registry.paths().collect();
        for (id, _) in ctx.struct_table.entries() {
            let owner = ctx
                .struct_table
                .owner_path(id)
                .expect("every StructId's owner must round-trip to a path");
            // No special case for the root module: `ModuleInternTable::new()`
            // pre-interns it as id 0, so it is already in `paths()`. (Branching
            // on a hardcoded module-name string here would itself be an instance
            // of the debt this epic is retiring — see
            // `module_name_string_branches` in check_structural_debt_inventory.sh.)
            assert!(
                known.contains(owner),
                "struct owner {owner:?} is not a module of this program — \
                 StructRegistry::insert would have interned it lazily, in \
                 struct-registration order (Issue #11078)"
            );
        }
    }

    /// Compiling the SAME source twice in one process assigns identical
    /// `StructId`s (Issue #11078). Guards against any accidental dependence on
    /// `HashMap` iteration order (per-process-random via `RandomState`) or on
    /// leftover per-process state in the allocation path.
    #[test]
    fn struct_ids_are_identical_across_two_compiles_of_the_same_source_11078() {
        let ids = || {
            let program = parse_and_lower_ok(PARITY_CORPUS_10265);
            let compiled = crate::compile::compile_core_program(&program).expect("compile");
            let ctx = compiled.compile_context.as_ref().expect("compile context");
            struct_id_snapshot(&ctx.struct_table)
        };
        assert_eq!(ids(), ids());
    }
    /// Issue #10265 acceptance: fresh vs Base-bytecode-cached compile must
    /// produce the same `struct_table` semantics — the exact lane #10092
    /// regressed on (`pipeline_ctx::build_struct_tables` rebuilding cached
    /// Base entries with `has_inner_constructor: false`). `type_id`s may
    /// legitimately differ between the two lanes (the cached lane pre-seeds
    /// Base ids), so ids are checked structurally: each entry's id must index
    /// a `struct_defs` row of the same shape in ITS OWN program.
    #[test]
    fn base_cached_compile_struct_table_matches_fresh_compile_10265() {
        let program = parse_and_lower_ok(PARITY_CORPUS_10265);
        let prelude_function_count = crate::get_prelude_program()
            .map(|p| p.functions.len())
            .unwrap_or(program.base_function_count);
        assert!(
            !should_skip_base_cache_for_program(&program, prelude_function_count),
            "corpus must stay on the Base-cache lane; if it now triggers the \
             bypass, adjust the corpus so this test still exercises \
             build_struct_tables' cached branch"
        );
        let fresh = crate::compile::compile_core_program(&program).expect("fresh compile");
        let cached = compile_core_bundle_with_base_cache(
            &program,
            &program,
            &HashMap::new(),
            &HashMap::new(),
            None,
            None,
            None,
            None,
        )
        .expect("base-cached compile")
        .compiled;
        // Lane-independent normalized view: `type_id`s legitimately differ
        // between the two lanes, so every id (the entry's own target AND any
        // `ValueType::Struct(id)` field type) is resolved to the def NAME in
        // its own lane's `struct_defs`. This still catches the divergence
        // class where a bare alias points at a DIFFERENT struct per lane
        // (Issue #10341: bare `SpinLock` resolved to the top-level def under
        // cache but to `Threads.SpinLock` without it).
        fn normalized_struct_table(
            compiled: &CompiledProgram,
        ) -> Vec<(String, String, bool, Vec<(String, String)>, bool)> {
            let ctx = compiled.compile_context.as_ref().expect("context");
            let def_name = |id: usize| -> String {
                compiled
                    .struct_defs
                    .get(id)
                    .map(|d| d.name.clone())
                    .unwrap_or_else(|| format!("<out-of-bounds {id}>"))
            };
            let mut rows: Vec<_> = ctx
                .struct_table
                .iter()
                .map(|(name, info)| {
                    // Exhaustive destructuring (Issue #10265 guard): a new
                    // `StructInfo` field fails compilation here too.
                    let crate::compile::StructInfo {
                        type_id,
                        is_mutable,
                        fields,
                        has_inner_constructor,
                    } = info;
                    (
                        name.clone(),
                        def_name(*type_id),
                        *is_mutable,
                        fields
                            .iter()
                            .map(|(fname, vt)| {
                                let vt_repr = match vt {
                                    ValueType::Struct(id) => {
                                        format!("Struct({})", def_name(*id))
                                    }
                                    other => format!("{other:?}"),
                                };
                                (fname.clone(), vt_repr)
                            })
                            .collect::<Vec<_>>(),
                        *has_inner_constructor,
                    )
                })
                .collect();
            rows.sort();
            rows
        }
        assert_eq!(
            normalized_struct_table(&fresh),
            normalized_struct_table(&cached),
            "fresh and Base-cached compiles must agree on every struct_table \
             entry (name, target def, mutability, fields, \
             has_inner_constructor) — the #10092 regression surface"
        );
        // type_id structural consistency within each program.
        for (label, compiled) in [("fresh", &fresh), ("cached", &cached)] {
            let ctx = compiled.compile_context.as_ref().expect("context");
            for (name, info) in &ctx.struct_table {
                let def = compiled.struct_defs.get(info.type_id).unwrap_or_else(|| {
                    panic!(
                        "{label}: struct_table[{name}].type_id {} out of bounds",
                        info.type_id
                    )
                });
                assert_eq!(
                    def.fields, info.fields,
                    "{label}: struct_table[{name}] and struct_defs[{}] disagree on fields",
                    info.type_id
                );
            }
        }
    }
    /// Issue #10962, #10974: Base's own struct constructor-self-family
    /// origin (e.g. `Rational`'s raw `Rational{T}(num::T, den::T) where T`
    /// inner constructor, tagged `ConstructorSelfFamily::ExplicitParametricInner`
    /// when Base is compiled from source) must survive a REAL
    /// `serialize_base_cache` -> `deserialize_base_cache` round trip byte for
    /// byte identically — the exact `--precompile-base` / embedded-cache
    /// boundary. Before the carrier lived on `MethodTable` itself, this
    /// origin was tracked only in a transient, unserialized
    /// `SharedCompileContext` set and could not be checked this way at all.
    ///
    /// This is deliberately a structural check of the carrier surviving the
    /// cache boundary, not an end-to-end compile of a constructor call: the
    /// separate cross-table dynamic-dispatch gap tracked by #10969 (confirmed
    /// during this PR's investigation to reproduce identically with an
    /// uncached, freshly-compiled Base — i.e. it is NOT caused by cache
    /// identity loss) is out of scope here and must not be conflated with
    /// this test.
    #[test]
    fn base_constructor_self_family_survives_cache_round_trip_10962() {
        use crate::compile::precompile::{deserialize_base_cache, serialize_base_cache};

        clear_cache();
        let fresh_base =
            compile_base_functions_from_source().expect("Base source compilation must succeed");
        let fresh_rational_table = fresh_base
            .method_tables
            .get("Rational")
            .expect("Base must register a method table for Rational");
        assert!(
            fresh_rational_table.has_explicit_parametric_inner_constructors(),
            "freshly source-compiled Base must tag Rational's own explicit-parametric \
             inner constructor before any cache round trip"
        );

        let bytes = serialize_base_cache(
            &fresh_base.compiled,
            &fresh_base.method_tables,
            &fresh_base.closure_captures,
            &fresh_base.inference_results,
        )
        .expect("Base cache serialization must succeed");
        let restored =
            deserialize_base_cache(&bytes).expect("Base cache deserialization must succeed");
        let restored_rational_table = restored
            .method_tables
            .get("Rational")
            .expect("restored Base cache must still register a method table for Rational");
        assert!(
            restored_rational_table.has_explicit_parametric_inner_constructors(),
            "Rational's own explicit-parametric inner-constructor origin must survive a \
             real Base-cache serialize -> deserialize round trip (Issue #10962, #10974)"
        );

        let fresh_origins: std::collections::BTreeMap<usize, bool> = fresh_rational_table
            .methods
            .iter()
            .map(|m| {
                (
                    m.global_index,
                    fresh_rational_table.is_explicit_parametric_inner_constructor(m.global_index),
                )
            })
            .collect();
        let restored_origins: std::collections::BTreeMap<usize, bool> = restored_rational_table
            .methods
            .iter()
            .map(|m| {
                (
                    m.global_index,
                    restored_rational_table
                        .is_explicit_parametric_inner_constructor(m.global_index),
                )
            })
            .collect();
        assert_eq!(
            fresh_origins, restored_origins,
            "every Rational method's explicit-parametric-inner-constructor origin bit must \
             be identical before and after the Base-cache round trip"
        );

        clear_cache();
    }
}

#[cfg(test)]
mod imported_binding_cache_restore_tests {
    use super::*;

    #[test]
    fn nonselective_reexport_context_survives_cache_restore_11240() -> Result<(), String> {
        let program = super::tests::parse_and_lower_ok(
            r#"
module Origin11240
export value11240
const value11240 = 42
end

module Facade11240
using ..Origin11240
export value11240
end

using .Facade11240
value11240
"#,
        );
        let fresh = crate::compile::compile_core_program(&program)
            .map_err(|error| format!("fresh compile failed: {error:?}"))?;
        let fresh_bindings = fresh
            .compile_context
            .as_ref()
            .ok_or_else(|| "nonselective re-export must create a compile context".to_string())?
            .module_imported_bindings
            .clone();
        assert!(
            !fresh_bindings.is_empty(),
            "corpus must exercise live imported-binding metadata"
        );

        let bytes = super::super::precompile::cache_serialize(&fresh)
            .map_err(|error| format!("cache serialize failed: {error}"))?;
        let mut restored: crate::bytecode::CompiledProgram =
            super::super::precompile::cache_deserialize(&bytes)
                .map_err(|error| format!("cache deserialize failed: {error}"))?;
        restore_compile_context_from_program(&mut restored, &program);
        let restored_bindings = &restored
            .compile_context
            .as_ref()
            .ok_or_else(|| "restore must retain nonselective imported-binding context".to_string())?
            .module_imported_bindings;
        assert_eq!(restored_bindings, &fresh_bindings);
        Ok(())
    }
}
