//! String / regex special-case handlers extracted from `compile_call`
//! (Issue #6332): `occursin`, `match`, `eachmatch` (pre-match table), and
//! the post-struct-resolution `sprint` and `parse`/`tryparse` cases.
//!
//! Each handler preserves the original branch bodies and their relative
//! order for a given function name. `None` = fall through to the generic
//! method-dispatch path (identical to the original branches ending without
//! `return`).

use crate::builtins::BuiltinId;
use crate::bytecode::{Instr, ValueType};
use crate::compile::{err, CResult, CoreCompiler};
use crate::ir::core::Expr;

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

/// `match(regex, string)` or `match(regex, string, start)` — regex matching
/// builtin. The 3-arg form searches from a 1-based byte offset (Issue #10178).
pub(super) fn compile_regex_match(
    c: &mut CoreCompiler<'_>,
    ctx: &CallCtx<'_>,
) -> Option<CResult<ValueType>> {
    if !(2..=3).contains(&ctx.args.len()) || !ctx.kwargs.is_empty() {
        return None;
    }
    let first_arg_type = c.infer_expr_type(&ctx.args[0]);
    if first_arg_type != ValueType::Regex
        && c.method_tables.contains_key("match")
        && !c.in_base_function_scope
    {
        return None;
    }
    for arg in ctx.args {
        ctry!(c.compile_expr(arg));
    }
    c.emit(Instr::CallBuiltin(BuiltinId::RegexMatch, ctx.args.len()));
    Some(Ok(ValueType::Any)) // Returns RegexMatch or Nothing
}

/// `Regex(pattern)` or `Regex(pattern, flags)` — regex constructor builtin.
/// Routes the runtime-construction call to the `RegexNew` builtin so patterns
/// can be built dynamically (e.g. `Regex(escaped_user_input)`). Without this,
/// the single-arg form fell through the builtin-type-name constructor path and
/// errored `Unknown function: Regex` (Issue #10178). `RegexNew` is the only
/// `Regex` constructor (there is no pure-Julia `Regex` method), so both the 1-
/// and 2-arg forms route here unconditionally.
pub(super) fn compile_regex_new(
    c: &mut CoreCompiler<'_>,
    ctx: &CallCtx<'_>,
) -> Option<CResult<ValueType>> {
    if !(1..=2).contains(&ctx.args.len()) || !ctx.kwargs.is_empty() {
        return None;
    }
    for arg in ctx.args {
        ctry!(c.compile_expr(arg));
    }
    c.emit(Instr::CallBuiltin(BuiltinId::RegexNew, ctx.args.len()));
    Some(Ok(ValueType::Regex))
}

/// `eachmatch(regex, string; overlap=false)` — regex iteration builtin.
///
/// The optional `overlap` keyword (Issue #10199) is threaded to the runtime as
/// a third stack value; when `overlap=true` the builtin restarts the search one
/// character past each match start instead of past its end. Only the bare
/// 2-argument form and the single `overlap` keyword are special-cased here; any
/// other keyword (or a keyword splat) falls through to the generic call path.
pub(super) fn compile_regex_eachmatch(
    c: &mut CoreCompiler<'_>,
    ctx: &CallCtx<'_>,
) -> Option<CResult<ValueType>> {
    if ctx.args.len() != 2 {
        return Some(err(
            "eachmatch requires exactly 2 arguments: eachmatch(regex, string)",
        ));
    }
    if ctx.has_kwargs_splat {
        return None;
    }
    let overlap_expr = match ctx.kwargs {
        [] => None,
        [(name, expr)] if name == "overlap" => Some(expr),
        _ => return None,
    };
    ctry!(c.compile_expr(&ctx.args[0]));
    ctry!(c.compile_expr(&ctx.args[1]));
    if let Some(overlap_expr) = overlap_expr {
        ctry!(c.compile_expr(overlap_expr));
        c.emit(Instr::CallBuiltin(BuiltinId::RegexEachmatch, 3));
    } else {
        c.emit(Instr::CallBuiltin(BuiltinId::RegexEachmatch, 2));
    }
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
        Expr::Var(name, _) => c.method_tables.contains_key(name.as_str()),
        _ => false,
    };
    if !is_func_ref {
        return None;
    }
    // Check for context kwarg (Issue #334: IOContext support for sprint).
    // `sizehint` is upstream's preallocation hint with no observable effect
    // on the returned string (Issue #10364): accept it and drop it, so the
    // remaining kwarg set decides the route exactly as before.
    if ctx
        .kwargs
        .iter()
        .any(|(k, _)| k != "context" && k != "sizehint")
    {
        // Unknown kwargs keep the generic path (and its keyword error).
        return None;
    }
    let has_sizehint = ctx.kwargs.iter().any(|(k, _)| k == "sizehint");
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
        if matches!(&args[0], Expr::FunctionRef { name, .. } | Expr::Var(name, _) if name == "print" || name == "Base.print")
        {
            // The print fast path is excluded from the SprintFunc builtin;
            // with only a (dropped) sizehint kwarg, re-enter as a plain
            // positional `sprint` call so the pure-Julia method handles it
            // instead of the generic path erroring on the kwarg
            // (Issue #10364).
            if has_sizehint {
                return Some(c.compile_call("sprint", args, &[], &[], &[]));
            }
            return None;
        }
        // No context kwarg - use the fast builtin SprintFunc instruction
        // (sizehint, if present, is a dropped no-op hint).
        Some(c.compile_builtin_call(ctx.function, args))
    }
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
                    function: "_parse_int_base".to_string().into(),
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
