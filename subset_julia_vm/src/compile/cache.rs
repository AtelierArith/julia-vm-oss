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

use super::abstract_interp::engine::{CachedReturn, InferenceCacheKey};
use super::types::CResult;
use crate::ir::core::{Block, Expr, Module, Program, Stmt, TypeAliasDef};
use crate::vm::{CompiledProgram, RuntimeCompileContext, StructDefInfo, ValueType};
use std::cell::RefCell;
use std::collections::hash_map::DefaultHasher;
use std::collections::{HashMap, HashSet};
use std::env;
use std::fs;
use std::hash::{Hash, Hasher};
use std::io::ErrorKind;
use std::path::{Path, PathBuf};
use std::rc::Rc;
use std::time::{Duration, SystemTime};

/// Check if cache debug logging is enabled via environment variable
fn should_log_cache() -> bool {
    env::var("SUBSET_JULIA_VM_CACHE_DEBUG").is_ok()
}

/// Warm-start prefetch (Issue #6348, phase 2).
///
/// A one-shot CLI run spends its first ~16 ms on the main thread loading the
/// prelude `Program` and merging it with user code, then ~9 ms deserializing
/// the Base cache inside compile. The Base cache contains VM `Value`
/// constants (`Rc`-based, not `Send`), so it must stay on the compiling
/// thread — instead, `begin_warm_start_prefetch` moves the *prelude side* to
/// a background thread (`Program` is `Send`; the prelude already lives in a
/// sync `Lazy`): it warms the prelude Lazy and pre-clones the Base function
/// IR the shared inference engine consumes by value each compile. The CLI
/// then warms the Base cache on the main thread via `warm_base_cache` while
/// the background thread loads the prelude, overlapping the two largest
/// deserializes. Every consumer falls back to the regular path when the
/// prefetch is absent, failed, or mismatched.
#[cfg(not(target_arch = "wasm32"))]
mod warm_prefetch {
    use std::sync::Mutex;
    use std::thread::JoinHandle;

    pub(super) static INFERENCE_FNS_PREFETCH: Mutex<
        Option<JoinHandle<Option<Vec<crate::ir::core::Function>>>>,
    > = Mutex::new(None);

    pub(super) fn join<T>(slot: &Mutex<Option<JoinHandle<Option<T>>>>) -> Option<T> {
        let handle = slot.lock().ok()?.take()?;
        handle.join().ok()?
    }
}

/// Spawn a background thread that pre-loads warm-start artifacts
/// (Issue #6348): the prelude `Program` Lazy and a clone of the prelude
/// functions for the shared inference engine.
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
                Some(prelude.functions.clone())
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

/// Take the prefetched Base inference-function clones if they match the
/// expected Base segment length (at most once per process).
#[cfg(not(target_arch = "wasm32"))]
pub(super) fn take_prefetched_base_inference_functions(
    expected_len: usize,
) -> Option<Vec<crate::ir::core::Function>> {
    let funcs = warm_prefetch::join(&warm_prefetch::INFERENCE_FNS_PREFETCH)?;
    (funcs.len() == expected_len).then_some(funcs)
}

#[cfg(target_arch = "wasm32")]
pub(super) fn take_prefetched_base_inference_functions(
    _expected_len: usize,
) -> Option<Vec<crate::ir::core::Function>> {
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
        || program_extends_promotion_rules(program)
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
        Expr::UnaryOp { operand, .. } => {
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

/// Return true when user code extends promotion hooks whose callees live in cached Base bytecode.
///
/// Upstream Julia's `promote_type` calls `promote_rule` through ordinary multiple
/// dispatch (`julia/base/promotion.jl`). Cached sjulia Base bytecode cannot be
/// invalidated when a later user program adds a `promote_rule` method, so compile
/// the whole program in that case to keep the method table visible to `promote_type`
/// (Issue #4048).
fn program_extends_promotion_rules(program: &Program) -> bool {
    program
        .functions
        .iter()
        .skip(program.base_function_count)
        .any(|func| func.name == "promote_rule")
}

/// Return true when user code extends iterator trait hooks used by cached Base collect.
///
/// Cached Base bytecode cannot be invalidated when a later user program adds
/// `IteratorEltype` / `IteratorSize` methods. Full compilation keeps those methods
/// visible to `collect` and `_collect` dispatch (Issue #4088).
fn program_extends_iterator_traits(program: &Program) -> bool {
    program
        .functions
        .iter()
        .skip(program.base_function_count)
        .any(|func| matches!(func.name.as_str(), "IteratorEltype" | "IteratorSize"))
}

/// Return true when user code extends Dict view hooks used by cached Base dispatch.
///
/// `keys` / `values` / `pairs` are dispatch-first Base functions with retained
/// Rust-backed Dict fallbacks. Cached Base bytecode cannot see later user
/// extensions such as `keys(::Dict{String,Float64})`, so compile the whole
/// program in that case to keep runtime method visibility aligned (Issue #4671).
fn program_extends_dict_view_functions(program: &Program) -> bool {
    program
        .functions
        .iter()
        .skip(program.base_function_count)
        .any(|func| matches!(func.name.as_str(), "keys" | "values" | "pairs"))
}

/// Return true when user main contains block-local function definitions.
///
/// Cached Base bytecode carries method-table visibility from the precompiled
/// Base segment. A later block-local `Stmt::FunctionDef` in user main can be
/// referenced through generic Base helpers such as `@testset`; compiling the
/// whole program keeps those local methods visible in the same method table
/// used to compile the call site (Issue #8469).
fn program_main_contains_block_function_defs(program: &Program) -> bool {
    let user_function_names: HashSet<&str> = program
        .functions
        .iter()
        .skip(program.base_function_count)
        .map(|func| func.name.as_str())
        .collect();
    block_contains_function_def(&program.main, &user_function_names)
}

fn block_contains_function_def(block: &Block, user_function_names: &HashSet<&str>) -> bool {
    block
        .stmts
        .iter()
        .any(|stmt| stmt_contains_function_def(stmt, user_function_names))
}

fn stmt_contains_function_def(stmt: &Stmt, user_function_names: &HashSet<&str>) -> bool {
    match stmt {
        Stmt::FunctionDef { .. } => true,
        Stmt::Block(block)
        | Stmt::Timed { body: block, .. }
        | Stmt::TestSet { body: block, .. }
        | Stmt::While { body: block, .. }
        | Stmt::For { body: block, .. }
        | Stmt::ForEach { body: block, .. }
        | Stmt::ForEachTuple { body: block, .. } => {
            block_contains_function_def(block, user_function_names)
        }
        Stmt::If {
            condition,
            then_branch,
            else_branch,
            ..
        } => {
            expr_contains_function_def(condition, user_function_names)
                || block_contains_function_def(then_branch, user_function_names)
                || else_branch
                    .as_ref()
                    .is_some_and(|block| block_contains_function_def(block, user_function_names))
        }
        Stmt::Try {
            try_block,
            catch_block,
            else_block,
            finally_block,
            ..
        } => {
            block_contains_function_def(try_block, user_function_names)
                || catch_block
                    .as_ref()
                    .is_some_and(|block| block_contains_function_def(block, user_function_names))
                || else_block
                    .as_ref()
                    .is_some_and(|block| block_contains_function_def(block, user_function_names))
                || finally_block
                    .as_ref()
                    .is_some_and(|block| block_contains_function_def(block, user_function_names))
        }
        Stmt::Assign { value, .. } | Stmt::AddAssign { value, .. } => {
            expr_contains_function_def(value, user_function_names)
        }
        Stmt::Return { value, .. } => value
            .as_ref()
            .is_some_and(|expr| expr_contains_function_def(expr, user_function_names)),
        Stmt::Expr { expr, .. }
        | Stmt::Test {
            condition: expr, ..
        } => expr_contains_function_def(expr, user_function_names),
        Stmt::TestThrows { expr, .. } => expr_contains_function_def(expr, user_function_names),
        Stmt::IndexAssign { indices, value, .. } => {
            indices
                .iter()
                .any(|expr| expr_contains_function_def(expr, user_function_names))
                || expr_contains_function_def(value, user_function_names)
        }
        Stmt::FieldAssign { value, .. } => expr_contains_function_def(value, user_function_names),
        Stmt::DestructuringAssign { value, .. } => {
            expr_contains_function_def(value, user_function_names)
        }
        Stmt::DictAssign { key, value, .. } => {
            expr_contains_function_def(key, user_function_names)
                || expr_contains_function_def(value, user_function_names)
        }
        Stmt::EvalFunctionDef { .. }
        | Stmt::Break { .. }
        | Stmt::Continue { .. }
        | Stmt::Meta { .. }
        | Stmt::Using { .. }
        | Stmt::Export { .. }
        | Stmt::Label { .. }
        | Stmt::Goto { .. }
        | Stmt::EnumDef { .. }
        | Stmt::Global { .. } => false,
    }
}

fn expr_contains_function_def(expr: &Expr, user_function_names: &HashSet<&str>) -> bool {
    match expr {
        Expr::BinaryOp { left, right, .. } => {
            expr_contains_function_def(left, user_function_names)
                || expr_contains_function_def(right, user_function_names)
        }
        Expr::UnaryOp { operand, .. }
        | Expr::FieldAccess {
            object: operand, ..
        }
        | Expr::QuoteLiteral {
            constructor: operand,
            ..
        }
        | Expr::AssignExpr { value: operand, .. } => {
            expr_contains_function_def(operand, user_function_names)
        }
        Expr::Call { args, kwargs, .. } | Expr::ModuleCall { args, kwargs, .. } => {
            args.iter()
                .any(|expr| expr_contains_function_def(expr, user_function_names))
                || kwargs
                    .iter()
                    .any(|(_, value)| expr_contains_function_def(value, user_function_names))
        }
        Expr::Builtin { args, .. } | Expr::ArrayLiteral { elements: args, .. } => args
            .iter()
            .any(|expr| expr_contains_function_def(expr, user_function_names)),
        Expr::Index { array, indices, .. } => {
            expr_contains_function_def(array, user_function_names)
                || indices
                    .iter()
                    .any(|expr| expr_contains_function_def(expr, user_function_names))
        }
        Expr::Range {
            start, step, stop, ..
        } => {
            expr_contains_function_def(start, user_function_names)
                || step
                    .as_ref()
                    .is_some_and(|step| expr_contains_function_def(step, user_function_names))
                || expr_contains_function_def(stop, user_function_names)
        }
        Expr::Comprehension {
            body, iter, filter, ..
        }
        | Expr::Generator {
            body, iter, filter, ..
        } => {
            expr_contains_function_def(body, user_function_names)
                || expr_contains_function_def(iter, user_function_names)
                || filter
                    .as_ref()
                    .is_some_and(|filter| expr_contains_function_def(filter, user_function_names))
        }
        Expr::MultiComprehension {
            body,
            iterations,
            filter,
            ..
        } => {
            expr_contains_function_def(body, user_function_names)
                || iterations
                    .iter()
                    .any(|(_, iter)| expr_contains_function_def(iter, user_function_names))
                || filter
                    .as_ref()
                    .is_some_and(|filter| expr_contains_function_def(filter, user_function_names))
        }
        Expr::TupleLiteral { elements, .. }
        | Expr::StringConcat {
            parts: elements, ..
        } => elements
            .iter()
            .any(|expr| expr_contains_function_def(expr, user_function_names)),
        Expr::NamedTupleLiteral { fields, .. } => fields
            .iter()
            .any(|(_, value)| expr_contains_function_def(value, user_function_names)),
        Expr::Pair { key, value, .. } => {
            expr_contains_function_def(key, user_function_names)
                || expr_contains_function_def(value, user_function_names)
        }
        Expr::DictLiteral { pairs, .. } => pairs.iter().any(|(key, value)| {
            expr_contains_function_def(key, user_function_names)
                || expr_contains_function_def(value, user_function_names)
        }),
        Expr::LetBlock { bindings, body, .. } => {
            bindings
                .iter()
                .any(|(_, value)| expr_contains_function_def(value, user_function_names))
                || block_contains_function_def(body, user_function_names)
        }
        Expr::Ternary {
            condition,
            then_expr,
            else_expr,
            ..
        } => {
            expr_contains_function_def(condition, user_function_names)
                || expr_contains_function_def(then_expr, user_function_names)
                || expr_contains_function_def(else_expr, user_function_names)
        }
        Expr::New { args, .. } => args
            .iter()
            .any(|expr| expr_contains_function_def(expr, user_function_names)),
        Expr::DynamicTypeConstruct {
            base_expr,
            type_args,
            ..
        } => {
            base_expr
                .as_ref()
                .is_some_and(|base| expr_contains_function_def(base, user_function_names))
                || type_args
                    .iter()
                    .any(|expr| expr_contains_function_def(expr, user_function_names))
        }
        Expr::ReturnExpr { value, .. } => value
            .as_ref()
            .is_some_and(|value| expr_contains_function_def(value, user_function_names)),
        Expr::FunctionRef { name, .. } => user_function_names.contains(name.as_str()),
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
}

/// Record that `program_hash` was compiled once; returns `true` when this is
/// at least the second compile (i.e. the result is worth storing).
fn program_cache_should_store(program_hash: u64) -> bool {
    PROGRAM_CACHE_SEEN.with(|seen| !seen.borrow_mut().insert(program_hash))
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
        method_tables: cache.method_tables,
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

pub(crate) fn restore_compile_context_from_program(
    compiled: &mut CompiledProgram,
    program: &Program,
) {
    if compiled.compile_context.is_some()
        || !program_needs_restored_compile_context(compiled, program)
    {
        return;
    }

    let mut parametric_structs = HashMap::new();
    let mut has_inner_constructor = HashMap::new();
    for def in &program.structs {
        has_inner_constructor.insert(def.name.clone(), !def.inner_constructors.is_empty());
        if !def.type_params.is_empty() {
            parametric_structs.insert(
                def.name.clone(),
                super::ParametricStructDef { def: def.clone() },
            );
        }
    }
    for module in &program.modules {
        register_restored_module_parametric_structs(&mut parametric_structs, module, "");
    }

    let struct_table = compiled
        .struct_defs
        .iter()
        .enumerate()
        .map(|(type_id, def)| {
            (
                def.name.clone(),
                super::StructInfo {
                    type_id,
                    is_mutable: def.is_mutable,
                    fields: def.fields.clone(),
                    has_inner_constructor: *has_inner_constructor.get(&def.name).unwrap_or(&false),
                },
            )
        })
        .collect();

    // Issue #6657: mirror the fresh-compile detection of a user `getindex`
    // override on a native array receiver from the source AST (no method tables
    // are available on this cache-restore path). User functions are those at
    // index >= base_function_count.
    let disable_array_getindex_specialization =
        program.functions.iter().enumerate().any(|(idx, f)| {
            idx >= program.base_function_count
                && matches!(f.name.as_str(), "getindex" | "Base.getindex")
                && f.params
                    .first()
                    .and_then(|p| p.type_annotation.as_ref())
                    .is_some_and(julia_type_is_array_like_receiver)
        });

    // Issue #6806: the same fresh-compile detection for a user `setindex!`
    // override on a native array receiver — `setindex!(a, v, i)` has the array
    // receiver at param 0, like `getindex` — so the IndexStore write fast path is
    // refused for such programs.
    let disable_array_setindex_specialization =
        program.functions.iter().enumerate().any(|(idx, f)| {
            idx >= program.base_function_count
                && matches!(f.name.as_str(), "setindex!" | "Base.setindex!")
                && f.params
                    .first()
                    .and_then(|p| p.type_annotation.as_ref())
                    .is_some_and(julia_type_is_array_like_receiver)
        });

    // Issue #8127: mirror the fresh-compile detection of a user `getproperty`
    // override (any user-origin `getproperty`/`Base.getproperty` method) from the
    // source AST. When present, the specializer skips its direct-`GetField` fast
    // path so `obj.field` reads reach the override via interpreter dispatch.
    let disable_field_access_specialization =
        program.functions.iter().enumerate().any(|(idx, f)| {
            idx >= program.base_function_count
                && matches!(f.name.as_str(), "getproperty" | "Base.getproperty")
        });

    let mut type_aliases = HashMap::new();
    for alias in &program.type_aliases {
        register_restored_type_alias(&mut type_aliases, alias);
    }
    for module in &program.modules {
        register_restored_module_type_aliases(&mut type_aliases, module, "");
    }

    compiled.compile_context = Some(RuntimeCompileContext {
        struct_table,
        struct_defs: compiled.struct_defs.clone(),
        parametric_structs,
        type_aliases,
        // User primitive types from the (deserialized) compiled program, so the
        // reconstructed context keeps them visible to type reflection (Issue #5058).
        primitive_types: compiled.primitive_types.clone(),
        disable_array_getindex_specialization,
        disable_array_setindex_specialization,
        disable_field_access_specialization,
    });
}

fn program_needs_restored_compile_context(compiled: &CompiledProgram, program: &Program) -> bool {
    !compiled.specializable_functions.is_empty()
        || !compiled.primitive_types.is_empty()
        || !program.primitive_types.is_empty()
        || !program.type_aliases.is_empty()
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
        || !module.primitive_types.is_empty()
        || module.structs.iter().any(|def| !def.type_params.is_empty())
        || module
            .submodules
            .iter()
            .any(module_needs_restored_compile_context)
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
        parametric_structs.insert(
            format!("{}.{}", module_path, def.name),
            super::ParametricStructDef { def: def.clone() },
        );
    }

    for submodule in &module.submodules {
        register_restored_module_parametric_structs(parametric_structs, submodule, &module_path);
    }
}

/// Whether a `JuliaType` parameter annotation is a native array-like receiver
/// (`Array`/`Vector`/`Matrix` or an abstract array family) — the source-AST
/// analogue of [`crate::compile::method_table::core_type_is_array_like`] used on
/// the cache-restore path (Issue #6657).
fn julia_type_is_array_like_receiver(ty: &crate::types::JuliaType) -> bool {
    use crate::types::JuliaType;
    match ty {
        JuliaType::Array
        | JuliaType::VectorOf(_)
        | JuliaType::MatrixOf(_)
        | JuliaType::AbstractArray => true,
        JuliaType::Struct(name) => {
            let base = name.split('{').next().unwrap_or(name);
            let base = base.rsplit('.').next().unwrap_or(base);
            matches!(
                base,
                "Array"
                    | "Vector"
                    | "Matrix"
                    | "AbstractArray"
                    | "AbstractVector"
                    | "AbstractMatrix"
                    | "DenseArray"
            )
        }
        _ => false,
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
    } = super::compile_core_program_internal(
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
/// - Struct type: `Stmt::Expr { expr: Builtin { TypeOf, [Literal::Str("Complex{Float64}")] } }` → `"Complex{Float64}"`
fn extract_return_type_from_promote_rule_body(body: &crate::ir::core::Block) -> Option<String> {
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
        Expr::Var(name, _) => Some(name.clone()),
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
fn extract_promotion_rules_from_ir(functions: &[crate::ir::core::Function]) {
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

    // Compile with precompiled Base bytecode AND cached method tables + closure captures (Option A!)
    let output = super::profile::time("cache.compile_core_program_internal", || {
        super::compile_core_program_internal(
            program,
            global_types,
            global_struct_names,
            super::CompilerCacheInput {
                precompiled_base: Some(&base_cache.compiled),
                method_tables: Some(&base_cache.method_tables),
                closure_captures: Some(&base_cache.closure_captures),
                inference_results: Some(&base_cache.inference_results),
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

#[cfg(test)]
mod tests {
    use super::*;
    use crate::builtins::BuiltinId;
    use crate::compile::promotion;
    use crate::ir::core::{Block, Expr, Function, KwParam, Literal, Module, Stmt, TypeAliasDef};
    use crate::span::Span;
    use crate::types::JuliaType;
    use crate::vm::{CompiledProgram, Instr, SpecializableFunction, StructDefInfo, ValueType};

    fn parse_and_lower_ok(src: &str) -> Program {
        crate::pipeline::parse_and_lower(src).expect("pipeline error")
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
        }
    }

    fn empty_module(name: &str, type_aliases: Vec<TypeAliasDef>) -> Module {
        Module {
            name: name.to_string(),
            is_bare: false,
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
            functions: vec![],
            struct_defs: vec![],
            abstract_types: vec![],
            primitive_types: vec![],
            show_methods: vec![],
            entry: 0,
            specializable_functions: vec![SpecializableFunction {
                ir: empty_function("f"),
                name: "f".to_string(),
                fallback_index: 0,
            }],
            runtime_specialization_map: vec![],
            compile_context: None,
            base_function_count: 0,
            macro_bindings: std::collections::HashMap::new(),
            global_slot_names: vec![],
            global_slot_types: vec![],
            global_slot_count: 0,
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
        assert!(!ctx.parametric_structs.contains_key("Box"));
        assert!(!ctx.parametric_structs.contains_key("PairBox"));
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
                    let derived = crate::vm::derived_runtime_signature(func, arity);
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
    fn test_should_skip_base_cache_for_user_promote_rule_extension() {
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
        assert!(should_skip_base_cache_for_program(
            &program,
            prelude.functions.len()
        ));
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
        program.functions = vec![base_retry, empty_function("check")];
        program.base_function_count = 1;

        assert!(program_user_functions_shadow_base_kwparams(&program));
        assert!(should_skip_base_cache_for_program(&program, 1));
    }

    #[test]
    fn test_should_not_skip_base_cache_for_non_shadowing_user_function_8469() {
        let mut base_retry = empty_function("retry");
        base_retry.kwparams = vec![kwparam("check")];

        let mut program = minimal_program(vec![], vec![]);
        program.functions = vec![base_retry, empty_function("not_check")];
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
        program.functions = vec![base_retry];
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
        program.functions = vec![base_retry];
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
        program.functions = vec![base_retry];
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

        let mut program = minimal_program(vec![], vec![]);
        program.functions = vec![base_retry, empty_function("main#anon")];
        program.base_function_count = 1;
        program.main.stmts = vec![Stmt::Expr {
            expr: Expr::FunctionRef {
                name: "main#anon".to_string(),
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
        program.functions = vec![base_retry];
        program.base_function_count = 1;
        program.main.stmts = vec![Stmt::Expr {
            expr: Expr::FunctionRef {
                name: "retry".to_string(),
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
            compiled.code.iter().any(|instr| matches!(instr, Instr::CallDynamic(_, 1, candidates)
                if candidates.iter().any(|c| matches!(c,
                    crate::vm::DynamicCallCandidate::Method(idx)
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
                .any(|instr| matches!(instr, Instr::CallDynamic(_, 1, candidates)
                    if candidates.contains(&crate::vm::DynamicCallCandidate::NativeIterator(
                        crate::vm::NativeIteratorKind::Generator)))),
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
        assert!(take_prefetched_base_inference_functions(prelude_len).is_none());

        begin_warm_start_prefetch();
        let funcs = take_prefetched_base_inference_functions(prelude_len)
            .expect("prefetched base inference functions must match the prelude length");
        assert_eq!(funcs.len(), prelude_len);

        // Consumed at most once.
        assert!(take_prefetched_base_inference_functions(prelude_len).is_none());

        // A length mismatch (e.g. Base-redefinition merge) must be rejected.
        begin_warm_start_prefetch();
        assert!(take_prefetched_base_inference_functions(prelude_len + 1).is_none());
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
}
