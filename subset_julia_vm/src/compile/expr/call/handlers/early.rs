//! Earliest special-case handlers extracted from `compile_call`
//! (Issue #6332): compiler-internal metadata wrappers
//! (`#__sjulia_boundscheck_enabled__`, `#__sjulia_inbounds__`,
//! `#__sjulia_inline__` / `#__sjulia_noinline__`), `print` / `println`
//! Any-argument routing, the `hasmethod(...; world=...)` form, `invoke`,
//! and the NamedTuple `merge` fast path.
//!
//! These originally lived in the if-chain at the very top of `compile_call`,
//! before the enum-constructor pre-pass, the splat block, and the
//! callable-variable resolution. They are dispatched from
//! [`super::early_special_case_handler`] at exactly that position; `None`
//! falls through to the enum/splat/callable-variable blocks, identical to
//! the original failed `if` conditions.

use crate::builtins::BuiltinId;
use crate::compile::{err, CResult, CoreCompiler};
use crate::types::JuliaType;
use crate::vm::{Instr, ValueType};

use super::super::is_typemax_uint64_call;
use super::{ctry, CallCtx};

/// `#__sjulia_boundscheck_enabled__` — push the current boundscheck flag.
pub(super) fn compile_boundscheck_enabled(
    c: &mut CoreCompiler<'_>,
    ctx: &CallCtx<'_>,
) -> Option<CResult<ValueType>> {
    if !ctx.args.is_empty() || !ctx.kwargs.is_empty() || ctx.has_splat || ctx.has_kwargs_splat {
        return Some(err("#__sjulia_boundscheck_enabled__ takes no arguments"));
    }
    c.emit(Instr::PushBoundsCheckEnabled);
    Some(Ok(ValueType::Bool))
}

/// `#__sjulia_inbounds__` — compile the wrapped expression with
/// `inbounds_context` set, restoring the previous flag afterwards.
pub(super) fn compile_inbounds(
    c: &mut CoreCompiler<'_>,
    ctx: &CallCtx<'_>,
) -> Option<CResult<ValueType>> {
    if ctx.args.len() != 1 || !ctx.kwargs.is_empty() || ctx.has_splat || ctx.has_kwargs_splat {
        return Some(err(
            "#__sjulia_inbounds__ requires exactly one unsplatted argument",
        ));
    }
    let previous = c.inbounds_context;
    c.inbounds_context = true;
    let result = c.compile_expr(&ctx.args[0]);
    c.inbounds_context = previous;
    Some(result)
}

/// `#__sjulia_inline__` / `#__sjulia_noinline__` — transparent wrappers.
pub(super) fn compile_inline_metadata(
    c: &mut CoreCompiler<'_>,
    ctx: &CallCtx<'_>,
) -> Option<CResult<ValueType>> {
    if ctx.args.len() != 1 || !ctx.kwargs.is_empty() || ctx.has_splat || ctx.has_kwargs_splat {
        return Some(err(
            "inline metadata wrappers require exactly one unsplatted argument",
        ));
    }
    Some(c.compile_expr(&ctx.args[0]))
}

/// `print` / `println` (and `Base.`-qualified forms) with an Any-typed
/// argument route straight to the builtin I/O compiler.
///
/// Issue #4580: when an argument is only known as Any, method-table
/// dispatch can pick a singleton fallback such as `Nothing` and then
/// try to coerce the runtime expression before printing. The builtin
/// I/O compiler already prints arbitrary values through PrintAny.
pub(super) fn compile_print_println(
    c: &mut CoreCompiler<'_>,
    ctx: &CallCtx<'_>,
) -> Option<CResult<ValueType>> {
    let io_builtin_name = match ctx.function {
        "print" | "Base.print" => "print",
        "println" | "Base.println" => "println",
        _ => unreachable!("guarded by handler registration"),
    };
    if ctx.kwargs.is_empty()
        && !ctx.has_splat
        && !ctx.has_kwargs_splat
        && ctx
            .args
            .iter()
            .any(|arg| matches!(c.infer_julia_type(arg), JuliaType::Any))
    {
        return Some(c.compile_builtin_call(io_builtin_name, ctx.args));
    }
    None
}

/// `hasmethod(f, types[, kwnames]; world=...)` — the world-kwarg form.
pub(super) fn compile_hasmethod_world(
    c: &mut CoreCompiler<'_>,
    ctx: &CallCtx<'_>,
) -> Option<CResult<ValueType>> {
    if ctx.kwargs.len() != 1 || ctx.kwargs[0].0 != "world" || ctx.has_splat || ctx.has_kwargs_splat
    {
        return None;
    }
    let args = ctx.args;
    if !(args.len() == 2 || args.len() == 3) {
        return Some(err(
            "hasmethod requires 2 or 3 arguments: hasmethod(f, types[, kwnames])",
        ));
    }
    for arg in args {
        ctry!(c.compile_expr(arg));
    }
    ctry!(c.compile_expr(&ctx.kwargs[0].1));
    if args.len() == 3 && is_typemax_uint64_call(&ctx.kwargs[0].1) {
        c.emit(Instr::PushStr(
            "code reflection cannot be used from generated functions".to_string(),
        ));
        c.emit(Instr::ThrowError);
        return Some(Ok(ValueType::Bool));
    }
    c.emit(Instr::CallBuiltin(BuiltinId::HasMethod, args.len() + 1));
    Some(Ok(ValueType::Bool))
}

/// `invoke` / `Base.invoke` (non-splat calls only).
pub(super) fn compile_invoke(
    c: &mut CoreCompiler<'_>,
    ctx: &CallCtx<'_>,
) -> Option<CResult<ValueType>> {
    if ctx.has_splat {
        return None;
    }
    Some(c.compile_invoke_call(ctx.args, ctx.kwargs, ctx.kwargs_splat_mask))
}

/// `merge` / `Base.merge` — NamedTuple merge fast path when no specific
/// user runtime method takes precedence.
pub(super) fn compile_merge(
    c: &mut CoreCompiler<'_>,
    ctx: &CallCtx<'_>,
) -> Option<CResult<ValueType>> {
    if !(ctx.kwargs.is_empty() && !ctx.has_splat && !ctx.has_kwargs_splat) {
        return None;
    }
    let names = match ctx.function {
        "merge" => &["merge", "Base.merge"][..],
        "Base.merge" => &["Base.merge", "merge"][..],
        _ => unreachable!("guarded by handler registration"),
    };
    if c.specific_runtime_candidates_for_names(names, ctx.args.len())
        .is_empty()
    {
        if let Some(result) = ctry!(c.try_compile_named_tuple_merge(ctx.args)) {
            return Some(Ok(result));
        }
    }
    None
}
