//! String / regex special-case handlers extracted from `compile_call`
//! (Issue #6332): `occursin`, `match`, `eachmatch` (pre-match table), and
//! the post-struct-resolution `sprint`, `string(x; base=N)`, and
//! `parse`/`tryparse` cases.
//!
//! Each handler preserves the original branch bodies and their relative
//! order for a given function name. `None` = fall through to the generic
//! method-dispatch path (identical to the original branches ending without
//! `return`).

use crate::builtins::BuiltinId;
use crate::compile::{err, CResult, CoreCompiler};
use crate::ir::core::Expr;
use crate::vm::{Instr, ValueType};

use super::{ctry, CallCtx};

/// 2-arg occursin: Regex case uses builtin; String/curried form falls
/// through to Pure Julia method dispatch (Issue #2563).
pub(super) fn compile_occursin(
    c: &mut CoreCompiler<'_>,
    ctx: &CallCtx<'_>,
) -> Option<CResult<ValueType>> {
    if ctx.args.len() != 2 {
        return None;
    }
    let first_arg_type = c.infer_expr_type(&ctx.args[0]);
    if first_arg_type == ValueType::Regex {
        // Regex occursin requires Rust builtin
        ctry!(c.compile_expr(&ctx.args[0]));
        ctry!(c.compile_expr(&ctx.args[1]));
        c.emit(Instr::CallBuiltin(BuiltinId::Occursin, 2));
        return Some(Ok(ValueType::Bool));
    }
    None
}

/// `match(regex, string)` — regex matching builtin.
pub(super) fn compile_regex_match(
    c: &mut CoreCompiler<'_>,
    ctx: &CallCtx<'_>,
) -> Option<CResult<ValueType>> {
    if ctx.args.len() != 2 {
        return None;
    }
    let first_arg_type = c.infer_expr_type(&ctx.args[0]);
    if first_arg_type != ValueType::Regex
        && c.method_tables.contains_key("match")
        && !c.in_base_function_scope
    {
        return None;
    }
    ctry!(c.compile_expr(&ctx.args[0]));
    ctry!(c.compile_expr(&ctx.args[1]));
    c.emit(Instr::CallBuiltin(BuiltinId::RegexMatch, 2));
    Some(Ok(ValueType::Any)) // Returns RegexMatch or Nothing
}

/// `eachmatch(regex, string)` — regex iteration builtin.
pub(super) fn compile_regex_eachmatch(
    c: &mut CoreCompiler<'_>,
    ctx: &CallCtx<'_>,
) -> Option<CResult<ValueType>> {
    if ctx.args.len() != 2 {
        return Some(err(
            "eachmatch requires exactly 2 arguments: eachmatch(regex, string)",
        ));
    }
    ctry!(c.compile_expr(&ctx.args[0]));
    ctry!(c.compile_expr(&ctx.args[1]));
    c.emit(Instr::CallBuiltin(BuiltinId::RegexEachmatch, 2));
    Some(Ok(ValueType::Array)) // Returns Array of RegexMatch
}

/// Special case: sprint(f, args...) with function reference as first argument.
/// When the first argument is a FunctionRef (lambda or named function reference),
/// use the builtin_hof handler which can actually call the function.
/// This takes precedence over the Pure Julia sprint(x) implementation which
/// would just convert the function to its string representation.
/// See Issue #402: sprint(f, args...) only works with user-defined functions.
pub(super) fn compile_sprint(
    c: &mut CoreCompiler<'_>,
    ctx: &CallCtx<'_>,
) -> Option<CResult<ValueType>> {
    let args = ctx.args;
    if args.is_empty() {
        return None;
    }
    let is_func_ref = match &args[0] {
        Expr::FunctionRef { .. } => true,
        Expr::Var(name, _) => c.method_tables.contains_key(name),
        _ => false,
    };
    if !is_func_ref {
        return None;
    }
    // Check for context kwarg (Issue #334: IOContext support for sprint)
    let context_kwarg = ctx.kwargs.iter().find(|(k, _)| k == "context");
    if let Some((_, context_expr)) = context_kwarg {
        // When context is provided, call sprint_context(f, args, context)
        // This routes to the Pure Julia implementation that respects IOContext properties
        let mut sprint_context_args = vec![args[0].clone()];

        // Create a tuple for the remaining args
        let remaining_args = if args.len() > 1 {
            Expr::TupleLiteral {
                elements: args[1..].to_vec(),
                span: args[0].span(),
            }
        } else {
            Expr::TupleLiteral {
                elements: vec![],
                span: args[0].span(),
            }
        };
        sprint_context_args.push(remaining_args);
        sprint_context_args.push(context_expr.clone());

        Some(c.compile_call("sprint_context", &sprint_context_args, &[], &[], &[]))
    } else {
        // No context kwarg - use the fast builtin SprintFunc instruction
        Some(c.compile_builtin_call(ctx.function, args))
    }
}

/// Special case: string(x; base=N) - integer to string with base (Issue #2036).
pub(super) fn compile_string_base_kwarg(
    c: &mut CoreCompiler<'_>,
    ctx: &CallCtx<'_>,
) -> Option<CResult<ValueType>> {
    if ctx.args.len() != 1 || ctx.kwargs.is_empty() {
        return None;
    }
    if let Some(base_expr) = ctx.kwargs.iter().find(|(k, _)| k == "base").map(|(_, v)| v) {
        ctry!(c.compile_expr(&ctx.args[0]));
        ctry!(c.compile_expr(base_expr));
        c.emit(Instr::CallBuiltin(BuiltinId::StringIntToBase, 2));
        return Some(Ok(ValueType::Str));
    }
    None
}

/// Special case: parse(T, s) and tryparse(T, s) - type parsing.
/// parse/tryparse for Int64/Bool/Float64 are now Pure Julia (base/parse.jl);
/// the Float64 methods call the `_tryparse_float64` intrinsic (libc strtod)
/// (Issue #6748). `parse(Int, s; base=N)` is now also Pure Julia: the kwargs
/// form is rewritten to a positional call to the pure-Julia `_parse_int_base`
/// helper (Issue #7875 / docs/COMPARISION.md P1), replacing the former
/// `StringToIntBase` Rust builtin. The base-parsing domain logic already lived
/// in pure Julia (`_tryparse_int`); only the kwarg extraction stays here.
pub(super) fn compile_parse_tryparse(
    c: &mut CoreCompiler<'_>,
    ctx: &CallCtx<'_>,
) -> Option<CResult<ValueType>> {
    let (function, args) = (ctx.function, ctx.args);
    if args.len() != 2 {
        return None;
    }
    // All types (Int64/Bool/Float64) fall through to pure-Julia method dispatch.
    // Rewrite parse(Int, s; base=N) into the positional pure-Julia helper call
    // `_parse_int_base(s, N)` so the base-parsing logic lives entirely in
    // base/parse.jl (no Rust builtin).
    if function == "parse" {
        let is_int_type = match &args[0] {
            Expr::Var(name, _) => {
                matches!(name.as_str(), "Int64" | "Int")
            }
            _ => false,
        };
        if is_int_type {
            if let Some(base_expr) = ctx.kwargs.iter().find(|(k, _)| k == "base").map(|(_, v)| v) {
                let rewritten = Expr::Call {
                    function: "_parse_int_base".to_string(),
                    args: vec![args[1].clone(), base_expr.clone()],
                    kwargs: Vec::new(),
                    splat_mask: Vec::new(),
                    kwargs_splat_mask: Vec::new(),
                    span: args[1].span(),
                };
                return Some(c.compile_expr(&rewritten));
            }
        }
    }
    None
}
