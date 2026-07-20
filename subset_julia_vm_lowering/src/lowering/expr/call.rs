//! Function call expression lowering.
//!
//! This module handles lowering of function calls, arrow functions,
//! do syntax, and argument list parsing.

#![deny(clippy::unwrap_used)]
#![deny(clippy::expect_used)]

use crate::error::{UnsupportedFeature, UnsupportedFeatureKind};
use crate::ir::core::{
    BinaryOp, Block, BuiltinOp, Expr, Function, KwParam, Stmt, TypedParam, UnaryOp,
};
use crate::lowering::function::{
    generate_default_arg_stubs, inject_kwarg_default_prologues,
    inject_parameter_destructuring_prologue, lower_anonymous_function_named, parse_parameter,
};
use crate::lowering::stmt::{lower_stmt, lower_stmt_with_ctx};
use crate::lowering::{internal_lowering_error, LambdaContext, LowerResult};
use crate::parser::cst::{CstWalker, Node, NodeKind};
use crate::stdlib;

use super::{is_broadcast_op, lower_expr, lower_expr_with_ctx, map_builtin_name};

/// Split a `new{...}` type-argument list on top-level commas, respecting nested
/// `()`, `{}`, and `[]` so that a parameter such as `Dict{Symbol, Any}` or
/// `elem_type(coefficient_ring(R))` is kept intact rather than torn apart at an
/// inner comma (Issue #7935).
fn split_top_level_type_args(s: &str) -> Vec<String> {
    let mut parts = Vec::new();
    let mut depth: i32 = 0;
    let mut current = String::new();
    for ch in s.chars() {
        match ch {
            '(' | '{' | '[' => {
                depth += 1;
                current.push(ch);
            }
            ')' | '}' | ']' => {
                depth -= 1;
                current.push(ch);
            }
            ',' if depth == 0 => {
                parts.push(std::mem::take(&mut current));
            }
            _ => current.push(ch),
        }
    }
    if !current.trim().is_empty() || !parts.is_empty() {
        parts.push(current);
    }
    parts
}

/// Classify a single `new{...}` type argument.
///
/// A bare identifier (`T`, `Int64`) stays a [`TypeExpr::TypeVar`] so the existing
/// `where`-clause type-variable / concrete-name handling applies. Anything more
/// complex — a call like `elem_type(R)` or a parametric application like
/// `Dict{Symbol, Any}` — is kept as a [`TypeExpr::RuntimeExpr`] holding the
/// source text, which the compiler re-lowers and evaluates at runtime to obtain
/// the concrete `DataType` (Issue #7935).
fn classify_new_type_arg(tok: &str) -> crate::types::TypeExpr {
    let is_plain_ident = !tok.is_empty()
        && tok
            .chars()
            .next()
            .is_some_and(|c| c.is_alphabetic() || c == '_')
        && tok.chars().all(|c| c.is_alphanumeric() || c == '_');
    if is_plain_ident {
        crate::types::TypeExpr::TypeVar(tok.to_string())
    } else {
        crate::types::TypeExpr::RuntimeExpr(tok.to_string())
    }
}

/// Build the dedicated `Expr::New` node from an already-lowered call argument
/// list. Both call-lowering lanes must use this seam: treating context-aware
/// `new{T}(x)` as an ordinary parametric callee produces a fictitious type
/// named `new{T}` instead of constructing the enclosing struct (Issue #11179).
fn lower_new_call_from_args(
    name: &str,
    args: Vec<Expr>,
    splat_mask: &[bool],
    span: crate::span::Span,
) -> Option<Expr> {
    if !is_new_call_name(name) {
        return None;
    }

    let type_args = if name.starts_with("new{") {
        let type_args_str = &name[4..name.len() - 1];
        split_top_level_type_args(type_args_str)
            .into_iter()
            .map(|arg| classify_new_type_arg(arg.trim()))
            .collect()
    } else {
        Vec::new()
    };
    // Expr::New deliberately has a compact whole-argument-list splat shape.
    // Normalize arbitrary positional/multiple splats through an ordinary tuple
    // call, then splat that single tuple into the struct allocator. This keeps
    // source evaluation order and every leading/interleaved argument instead
    // of retaining only the final splatted expression (Issues #11183/#11187).
    let (args, is_splat) = if splat_mask.iter().any(|is_splat| *is_splat) {
        (
            vec![Expr::Call {
                function: "tuple".into(),
                args,
                kwargs: Vec::new(),
                splat_mask: splat_mask.to_vec(),
                kwargs_splat_mask: Vec::new(),
                span,
            }],
            true,
        )
    } else {
        (args, false)
    };
    Some(Expr::New {
        type_args,
        args,
        is_splat,
        span,
    })
}

fn is_new_call_name(name: &str) -> bool {
    name == "new" || (name.starts_with("new{") && name.ends_with('}'))
}

/// Extract the operator string if the callee node represents an operator partial application.
///
/// Handles:
/// - Bare `NodeKind::Operator` (from `==(x)` parsed by primary.rs, Issue #3119)
/// - `NodeKind::ParenthesizedExpression { NodeKind::Operator }` (from `(==)(x)`)
///
/// Returns `Some(op_text)` for supported binary operators, `None` otherwise.
fn extract_partial_apply_operator<'a>(walker: &CstWalker<'a>, callee: Node<'a>) -> Option<String> {
    let op_text = match walker.kind(&callee) {
        NodeKind::Operator => walker.text(&callee).to_string(),
        NodeKind::ParenthesizedExpression => {
            let children = walker.named_children_vec(&callee);
            if children.len() == 1 && walker.kind(&children[0]) == NodeKind::Operator {
                walker.text(&children[0]).to_string()
            } else {
                return None;
            }
        }
        _ => return None,
    };
    // Exclude unary NOT and broadcast operators (handled elsewhere)
    if op_text == "!" || is_broadcast_op(&op_text) {
        return None;
    }
    // Only support operators that map to BinaryOp
    match op_text.as_str() {
        "==" | "!=" | ">" | "<" | ">=" | "<=" | "===" | "!==" => Some(op_text),
        _ => None,
    }
}

/// Returns `true` when an operator string names a regular Base function that may
/// be invoked in call position with two or more positional arguments (or a
/// splat): `+`, `-`, `*`, `/`, `\`, `%`, `^`, and the comparison operators.
///
/// Short-circuit operators (`&&`, `||`) and the `<:`/`>:` type operators are
/// excluded — they are syntactic forms, not ordinary callable functions, so a
/// `&&(a, b)` call must not be silently lowered to a function call (Issue #5144).
pub fn is_operator_function_call_target(op: &str) -> bool {
    matches!(
        op,
        "+" | "-"
            | "*"
            | "/"
            | "\\"
            | "%"
            | "^"
            | "<"
            | ">"
            | "<="
            | ">="
            | "=="
            | "!="
            | "==="
            | "!=="
    )
}

/// Returns `true` when an operator string is a syntactic form rather than an
/// ordinary callable function, so it must be rejected in call position
/// (Issue #5144, extended for Issue #10933):
///
/// - short-circuit `&&`/`||` (control flow, not functions),
/// - the `<:`/`>:` type operators (deliberately excluded by Issue #5144),
/// - assignment `=`, update-assignments (`+=`, `*=`, …), and `:=`,
/// - arrow `->`, splat `...`, type-assert `::`, field access `.`,
///   ternary `?`, and pair `=>` (lowered structurally, not via dispatch).
///
/// Every other operator token the parser produces (e.g. custom Unicode
/// operators like `⊗`, or Base operators like `≥`) names a regular function
/// and may be used as a call target (Issue #10933).
pub fn is_syntactic_operator(op: &str) -> bool {
    if matches!(
        op,
        "&&" | "||" | "<:" | ">:" | ":=" | "->" | "..." | "::" | "." | "?" | "=>"
    ) {
        return true;
    }
    // `=` and update-assignment operators (`+=`, `-=`, `//=`, `<<=`, …) end in
    // `=`; the callable comparison operators that also end in `=` are already
    // routed through `is_operator_function_call_target` before this check, but
    // exclude them here too so this predicate stands alone.
    op.ends_with('=') && !matches!(op, "==" | "!=" | "<=" | ">=" | "===" | "!==")
}

/// Extract the inner `ArrowFunctionExpression` from a `ParenthesizedExpression` callee,
/// for handling immediately invoked lambda expressions: `(x -> expr)(args)` (Issue #3142).
///
/// Returns `Some(arrow_node)` if the callee is `(arrow_function)`, `None` otherwise.
fn extract_paren_arrow_function<'a>(walker: &CstWalker<'a>, callee: Node<'a>) -> Option<Node<'a>> {
    if walker.kind(&callee) != NodeKind::ParenthesizedExpression {
        return None;
    }
    let children = walker.named_children_vec(&callee);
    if children.len() == 1 && walker.kind(&children[0]) == NodeKind::ArrowFunctionExpression {
        Some(children[0])
    } else {
        None
    }
}

fn wrap_expr_splat_args(
    args: Vec<Expr>,
    splat_mask: Vec<bool>,
    span: crate::span::Span,
) -> Vec<Expr> {
    args.into_iter()
        .enumerate()
        .map(|(idx, arg)| {
            if splat_mask.get(idx).copied().unwrap_or(false) {
                Expr::Builtin {
                    name: BuiltinOp::SplatInterpolation,
                    args: vec![arg],
                    span,
                }
            } else {
                arg
            }
        })
        .collect()
}

/// Lower an immediately invoked lambda expression `(x -> expr)(args)` inside a full-form
/// function body (no separate `LambdaContext`) by embedding the lambda as a nested
/// `FunctionDef` inside a `LetBlock` and calling it immediately (Issue #3142).
///
/// Produced IR:
/// ```text
/// LetBlock {
///   bindings: [],
///   body: [
///     Stmt::FunctionDef(__iife_N),   // defines the lambda
///     Stmt::Expr(Call(__iife_N, args)),  // calls it, yielding the block's value
///   ]
/// }
/// ```
///
/// The trailing statement is `Stmt::Expr` (the block's value), NOT `Stmt::Return`:
/// a `return` here would leak out of this `LetBlock` and tail-return the enclosing
/// function, so `r = (x -> body)(arg)` would return the lambda's value early and
/// drop the surrounding statement's continuation (Issue #8018).
fn lower_iife_as_nested<'a>(
    walker: &CstWalker<'a>,
    arrow_node: Node<'a>,
    args: Vec<Expr>,
    kwargs: Vec<(crate::ir::core::InternedStr, Expr)>,
    splat_mask: Vec<bool>,
    kwargs_splat_mask: Vec<bool>,
    span: crate::span::Span,
) -> LowerResult<Expr> {
    let arrow_span = walker.span(&arrow_node);
    let children = walker.named_children_vec(&arrow_node);
    let child_count = children.len();

    let mut params: Vec<TypedParam> = Vec::new();
    let mut kwparams: Vec<KwParam> = Vec::new();
    let mut defaults: Vec<Option<Expr>> = Vec::new();
    let mut body_expr: Option<Expr> = None;

    for (i, child) in children.iter().enumerate() {
        let is_last = i == child_count - 1;
        if is_last {
            body_expr = Some(lower_expr(walker, *child)?);
        } else {
            match walker.kind(child) {
                NodeKind::Identifier => {
                    let name = walker.text(child).to_string();
                    params.push(TypedParam::untyped(name, walker.span(child)));
                    defaults.push(None);
                }
                NodeKind::ArgumentList
                | NodeKind::TupleExpression
                | NodeKind::ParenthesizedExpression
                | NodeKind::ParameterList => {
                    collect_lifted_arrow_parameters(
                        walker,
                        *child,
                        &mut params,
                        &mut kwparams,
                        &mut defaults,
                    )?;
                }
                _ => {}
            }
        }
    }

    let body_expr = body_expr.ok_or_else(|| {
        UnsupportedFeature::new(
            UnsupportedFeatureKind::UnsupportedExpression(
                "immediately invoked lambda without body".to_string(),
            ),
            arrow_span,
        )
    })?;

    let iife_name = format!("__iife_{}", span.start);
    let func = Function {
        name: iife_name.clone(),
        params,
        kwparams,
        type_params: vec![],
        return_type: None,
        body: Block {
            stmts: vec![Stmt::Return {
                value: Some(body_expr),
                span: arrow_span,
            }],
            span: arrow_span,
        },
        is_base_extension: false,
        is_runtime_eval: false,
        span: arrow_span,
        new_struct_name: None,
    }
    .into_lowering_helper();

    let func = inject_kwarg_default_prologues(func);

    // Optional positional defaults: emit reduced-arity stubs so an IIFE invoked
    // with fewer args than declared binds the defaults (Issue #8047).
    let stubs = generate_default_arg_stubs(&func, &defaults);

    let func_def_stmt = Stmt::FunctionDef {
        func: Box::new(func),
        span,
    };

    let call_expr = Expr::Call {
        function: iife_name.into(),
        args,
        kwargs,
        splat_mask,
        kwargs_splat_mask,
        span,
    };

    let mut stmts = vec![func_def_stmt];
    for stub in stubs {
        stmts.push(Stmt::FunctionDef {
            func: Box::new(stub),
            span,
        });
    }
    stmts.push(Stmt::Expr {
        expr: call_expr,
        span,
    });

    Ok(Expr::LetBlock {
        bindings: vec![],
        body: Block { stmts, span },
        span,
    })
}

/// Lower an immediately-invoked **full-form** anonymous function
/// `(function(x) body end)(args)` by lifting the lambda into a `FunctionDef`
/// embedded directly in a `LetBlock` *body* and calling it by its generated name
/// in the same block.
///
/// The lambda's `FunctionDef` lives directly in the block `body`, matching the
/// arrow IIFE path `lower_iife_as_nested`. At the time of Issue #8030 this was
/// also required because the nested-function collector did not inspect
/// `LetBlock` binding values; it now does so for compositional expression
/// containers (Issue #11030), while this direct body shape remains the simplest
/// representation for an immediately invoked full-form function.
#[allow(clippy::too_many_arguments)]
fn lower_fullform_iife_as_nested<'a>(
    walker: &CstWalker<'a>,
    func_def_node: Node<'a>,
    lambda_ctx: Option<&LambdaContext>,
    args: Vec<Expr>,
    kwargs: Vec<(crate::ir::core::InternedStr, Expr)>,
    splat_mask: Vec<bool>,
    kwargs_splat_mask: Vec<bool>,
    span: crate::span::Span,
) -> LowerResult<Expr> {
    let (function, func) = lower_anonymous_function_named(walker, func_def_node, lambda_ctx)?;
    let func = func.into_lowering_helper();
    Ok(Expr::LetBlock {
        bindings: vec![],
        body: Block {
            stmts: vec![
                Stmt::FunctionDef {
                    func: Box::new(func),
                    span,
                },
                Stmt::Expr {
                    expr: Expr::Call {
                        function: function.into(),
                        args,
                        kwargs,
                        splat_mask,
                        kwargs_splat_mask,
                        span,
                    },
                    span,
                },
            ],
            span,
        },
        span,
    })
}

/// Lower an arrow function used as a value inside a full-form function body.
///
/// The no-context lowering path cannot add to the top-level `LambdaContext`, so
/// embed the lambda as a nested `FunctionDef` inside a `LetBlock`. This lets the
/// normal nested-function collector and closure-capture analysis see free
/// variables from the enclosing function (Issue #4289).
pub(super) fn lower_arrow_value_as_nested<'a>(
    walker: &CstWalker<'a>,
    arrow_node: Node<'a>,
) -> LowerResult<Expr> {
    lower_arrow_value_as_nested_impl(walker, arrow_node, None)
}

pub(super) fn lower_arrow_value_as_nested_with_ctx<'a>(
    walker: &CstWalker<'a>,
    arrow_node: Node<'a>,
    lambda_ctx: &LambdaContext,
) -> LowerResult<Expr> {
    lower_arrow_value_as_nested_impl(walker, arrow_node, Some(lambda_ctx))
}

/// Generate a closure for operator partial application: `op(x)` -> `y -> y op x` (Issue #3119).
fn lower_operator_partial_apply(
    op_text: &str,
    arg_expr: Expr,
    lambda_ctx: &LambdaContext,
    span: crate::span::Span,
) -> LowerResult<Expr> {
    let bin_op = match op_text {
        "==" => BinaryOp::Eq,
        "!=" => BinaryOp::Ne,
        ">" => BinaryOp::Gt,
        "<" => BinaryOp::Lt,
        ">=" => BinaryOp::Ge,
        "<=" => BinaryOp::Le,
        "===" => BinaryOp::Egal,
        "!==" => BinaryOp::NotEgal,
        _ => {
            return Err(UnsupportedFeature::new(
                UnsupportedFeatureKind::UnsupportedCallTarget,
                span,
            ))
        }
    };

    let lambda_name = lambda_ctx.next_lambda_name();
    let param_name = "__op_y".to_string();
    let body_expr = Expr::BinaryOp {
        op: bin_op,
        left: Box::new(Expr::Var(param_name.clone().into(), span)),
        right: Box::new(arg_expr),
        span,
    };

    let func = Function {
        name: lambda_name.clone(),
        params: vec![TypedParam::untyped(param_name, span)],
        kwparams: vec![],
        type_params: vec![],
        return_type: None,
        body: Block {
            stmts: vec![Stmt::Return {
                value: Some(body_expr),
                span,
            }],
            span,
        },
        is_base_extension: false,
        is_runtime_eval: false,
        span,
        new_struct_name: None,
    }
    .into_lowering_helper();

    lambda_ctx.add_lifted_function(func);

    Ok(Expr::FunctionRef {
        name: lambda_name.into(),
        span,
    })
}

fn lower_arrow_value_as_nested_impl<'a>(
    walker: &CstWalker<'a>,
    arrow_node: Node<'a>,
    lambda_ctx: Option<&LambdaContext>,
) -> LowerResult<Expr> {
    let span = walker.span(&arrow_node);
    let children = walker.named_children_vec(&arrow_node);
    let child_count = children.len();

    let mut params: Vec<TypedParam> = Vec::new();
    let mut kwparams: Vec<KwParam> = Vec::new();
    let mut defaults: Vec<Option<Expr>> = Vec::new();
    let mut body_expr: Option<Expr> = None;

    for (i, child) in children.iter().enumerate() {
        let is_last = i == child_count - 1;
        if is_last {
            let requires_value_context =
                crate::lowering::contains_value_parametric_base(walker, *child, &params, &kwparams);
            let requires_type_context =
                lambda_ctx.is_some_and(LambdaContext::has_active_type_params);
            body_expr = Some(
                match lambda_ctx.filter(|_| requires_value_context || requires_type_context) {
                    Some(ctx) => ctx.with_function_body_scope(|| {
                        ctx.with_active_value_params(&params, &kwparams, || {
                            lower_expr_with_ctx(walker, *child, ctx)
                        })
                    })?,
                    None => lower_expr(walker, *child)?,
                },
            );
        } else {
            match walker.kind(child) {
                NodeKind::Identifier => {
                    let name = walker.text(child).to_string();
                    params.push(TypedParam::untyped(name, walker.span(child)));
                    defaults.push(None);
                }
                NodeKind::ArgumentList
                | NodeKind::TupleExpression
                | NodeKind::ParenthesizedExpression
                | NodeKind::ParameterList => {
                    collect_lifted_arrow_parameters(
                        walker,
                        *child,
                        &mut params,
                        &mut kwparams,
                        &mut defaults,
                    )?;
                }
                _ => {}
            }
        }
    }

    let body_expr = body_expr.ok_or_else(|| {
        UnsupportedFeature::new(
            UnsupportedFeatureKind::UnsupportedExpression(
                "arrow function without body".to_string(),
            ),
            span,
        )
    })?;

    let lambda_name = lambda_ctx.map_or_else(
        || format!("__lambda_nested_{}", span.start),
        |context| context.nested_lambda_name(span.start),
    );
    let body = Block {
        stmts: vec![Stmt::Return {
            value: Some(body_expr),
            span,
        }],
        span,
    };
    let (params, body) = inject_parameter_destructuring_prologue(params, body);
    let func = Function {
        name: lambda_name.clone(),
        params,
        kwparams,
        type_params: vec![],
        return_type: None,
        body,
        is_base_extension: false,
        is_runtime_eval: false,
        span,
        new_struct_name: None,
    }
    .into_lowering_helper();

    let func = inject_kwarg_default_prologues(func);

    // Optional positional defaults bind in reduced-arity calls (Issue #8047).
    let stubs = generate_default_arg_stubs(&func, &defaults);

    let mut stmts = vec![Stmt::FunctionDef {
        func: Box::new(func),
        span,
    }];
    for stub in stubs {
        stmts.push(Stmt::FunctionDef {
            func: Box::new(stub),
            span,
        });
    }
    stmts.push(Stmt::Expr {
        expr: Expr::Var(lambda_name.into(), span),
        span,
    });

    Ok(Expr::LetBlock {
        bindings: vec![],
        body: Block { stmts, span },
        span,
    })
}

/// Generate a closure for operator partial application as a nested `FunctionDef` in a `LetBlock`
/// (Issue #3119). Used when lowering without a `LambdaContext` (inside full-form function bodies).
///
/// Returns `Expr::LetBlock { [FunctionDef(__partial_apply_N), Expr(Var(__partial_apply_N))] }`
/// so that `collect_stmt_functions` / `collect_expr_functions` can discover the definition as a
/// nested function of the enclosing function, enabling correct free variable / closure analysis
/// at compile time.
///
/// The lambda name is derived from `span.start` to guarantee uniqueness per source position
/// without requiring a shared counter.
fn lower_operator_partial_apply_as_nested(
    op_text: &str,
    arg_expr: Expr,
    span: crate::span::Span,
) -> LowerResult<Expr> {
    let bin_op = match op_text {
        "==" => BinaryOp::Eq,
        "!=" => BinaryOp::Ne,
        ">" => BinaryOp::Gt,
        "<" => BinaryOp::Lt,
        ">=" => BinaryOp::Ge,
        "<=" => BinaryOp::Le,
        "===" => BinaryOp::Egal,
        "!==" => BinaryOp::NotEgal,
        _ => {
            return Err(UnsupportedFeature::new(
                UnsupportedFeatureKind::UnsupportedCallTarget,
                span,
            ))
        }
    };

    // Use span.start as a unique discriminator (each partial application has a unique source pos).
    let lambda_name = format!("__partial_apply_{}", span.start);
    let param_name = "__op_y".to_string();

    // Lambda body: __op_y op arg_expr
    // arg_expr is inlined so free variable analysis detects captures from the enclosing scope.
    let body_expr = Expr::BinaryOp {
        op: bin_op,
        left: Box::new(Expr::Var(param_name.clone().into(), span)),
        right: Box::new(arg_expr),
        span,
    };

    let func = Function {
        name: lambda_name.clone(),
        params: vec![TypedParam::untyped(param_name, span)],
        kwparams: vec![],
        type_params: vec![],
        return_type: None,
        body: Block {
            stmts: vec![Stmt::Return {
                value: Some(body_expr),
                span,
            }],
            span,
        },
        is_base_extension: false,
        is_runtime_eval: false,
        span,
        new_struct_name: None,
    }
    .into_lowering_helper();

    // Embed as a nested FunctionDef inside a LetBlock so that:
    // 1. collect_stmt_functions discovers it as a nested function of the enclosing function.
    // 2. At runtime (strict_undefined_check=true), FunctionDef compilation stores the function
    //    (or a closure capturing free variables) into the local scope under `lambda_name`.
    // 3. The trailing Stmt::Expr loads `lambda_name` as the block's value.
    let func_def_stmt = Stmt::FunctionDef {
        func: Box::new(func),
        span,
    };
    let var_ref_stmt = Stmt::Expr {
        expr: Expr::Var(lambda_name.clone().into(), span),
        span,
    };
    Ok(Expr::LetBlock {
        bindings: vec![],
        body: Block {
            stmts: vec![func_def_stmt, var_ref_stmt],
            span,
        },
        span,
    })
}

/// Result of resolving a call target NodeKind (Issue #2271).
///
/// This enum eliminates the duplicated call target resolution logic between
/// `lower_call_expr_with_ctx` and `lower_call_expr`. Both functions share
/// the same call target resolution but differ in argument list lowering.
enum ResolvedCallTarget {
    /// Module-qualified call: Module.func(args)
    ModuleCall { module: String, function: String },
    /// Indirect call via FieldExpression or IndexExpression:
    /// obj.f(args) or tuple[1](args), lowered via a LetBlock with temp variable.
    IndirectCall { expr: Expr, temp_name: String },
    /// Direct call by name: func(args)
    DirectCall { name: String },
    /// Unary NOT operator: !(expr)
    UnaryNot { operand: Expr },
}

/// Split `Base.T{...}` into the same module/function ownership shape as
/// `Base.f(...)`. User-qualified parametric callees intentionally remain on
/// the direct-call path, which supports runtime type expressions in `M.T{expr}`.
fn split_base_parametric_call_target(name: &str) -> Option<(String, String)> {
    let brace = name.find('{')?;
    let head = &name[..brace];
    let (module, leaf) = head.rsplit_once('.')?;
    if module != "Base" || leaf.is_empty() {
        return None;
    }
    Some((module.to_string(), format!("{}{}", leaf, &name[brace..])))
}

/// Resolve the call target from a callee node (Issue #2271).
///
/// This is the single source of truth for call target NodeKind handling,
/// ensuring both `lower_call_expr_with_ctx` and `lower_call_expr` stay in sync.
fn resolve_call_target<'a>(
    walker: &CstWalker<'a>,
    callee: Node<'a>,
    named: &[Node<'a>],
    span: crate::span::Span,
) -> LowerResult<ResolvedCallTarget> {
    // FieldExpression: Module.func(args) or obj.f(args)
    if walker.kind(&callee) == NodeKind::FieldExpression {
        if let Some((module_name, func_name)) = extract_module_call_target(walker, callee) {
            return Ok(ResolvedCallTarget::ModuleCall {
                module: module_name,
                function: func_name,
            });
        } else {
            let field_expr = super::lower_field_expr(walker, callee)?;
            let temp_name = format!("__field_func_{}", span.start);
            return Ok(ResolvedCallTarget::IndirectCall {
                expr: field_expr,
                temp_name,
            });
        }
    }

    // IndexExpression: tuple[1](args) (Issue #2240)
    if walker.kind(&callee) == NodeKind::IndexExpression {
        let index_expr = super::lower_index_expr(walker, callee)?;
        let temp_name = format!("__indexed_func_{}", span.start);
        return Ok(ResolvedCallTarget::IndirectCall {
            expr: index_expr,
            temp_name,
        });
    }

    // Name extraction from Identifier, ParametrizedTypeExpression, Operator
    match walker.kind(&callee) {
        NodeKind::Identifier => Ok(ResolvedCallTarget::DirectCall {
            // Keep bare callee names intact here. Compile-time resolution routes
            // visible type aliases through DataType callables with lexical/module
            // visibility intact; expanding in lowering would leak module aliases
            // before `using M: x` filtering can run.
            name: walker.text(&callee).to_string(),
        }),
        NodeKind::ParametrizedTypeExpression => {
            // Issue #5055: expand a parametric type-alias constructor call head
            // (`MyVec{Int}([..])` -> `Vector{Int}([..])`). A qualified head is
            // a module call, not a flat direct-call string: retaining that
            // structure prevents `Base.Dict{K,V}` from being rebound to a
            // module-local `Dict` alias (Issue #11369).
            let name = crate::lowering::type_alias::expand(walker.text(&callee));
            if let Some((module, function)) = split_base_parametric_call_target(&name) {
                Ok(ResolvedCallTarget::ModuleCall { module, function })
            } else {
                Ok(ResolvedCallTarget::DirectCall { name })
            }
        }
        NodeKind::Operator => {
            let op_text = walker.text(&callee).to_string();

            // Unary NOT: !(expr)
            if op_text == "!" {
                let args_node = named.iter().skip(1).find(|n| {
                    matches!(
                        walker.kind(n),
                        NodeKind::ArgumentList | NodeKind::TupleExpression
                    )
                });

                if let Some(args) = args_node {
                    let arg_children = walker.named_children_vec(args);
                    if arg_children.len() == 1 {
                        let operand = lower_expr(walker, arg_children[0])?;
                        return Ok(ResolvedCallTarget::UnaryNot { operand });
                    }
                }
                return Err(UnsupportedFeature::new(
                    UnsupportedFeatureKind::UnsupportedCallTarget,
                    walker.span(&callee),
                ));
            }

            // Broadcast operators: .*(a, b), .+(a, b)
            if is_broadcast_op(&op_text) || matches!(op_text.as_str(), "∈" | "∉" | "∋" | "∌")
            {
                Ok(ResolvedCallTarget::DirectCall { name: op_text })
            } else if is_operator_function_call_target(&op_text) {
                // Operator used as a function in call position with multiple
                // arguments or a splat: `*(2, 3, 4)`, `^(2, 3)`, `*(xs...)`.
                // Single-argument operator partial application (`>(3)`) is
                // intercepted earlier by `extract_partial_apply_operator`, so by
                // the time we reach here the operator names a real Base function
                // that the method table resolves at runtime — mirror `min`/`max`
                // (Issue #5144).
                Ok(ResolvedCallTarget::DirectCall { name: op_text })
            } else if is_syntactic_operator(&op_text) {
                // Syntactic forms (`&&`, `=`, `->`, …) are not callable
                // functions; keep rejecting them in call position (Issue #5144).
                Err(UnsupportedFeature::new(
                    UnsupportedFeatureKind::UnsupportedCallTarget,
                    walker.span(&callee),
                ))
            } else {
                // Any other operator identifier is an ordinary function name,
                // exactly like upstream Julia: a user-definable (custom
                // Unicode) operator such as `⊗` accepted in definition
                // position is also accepted as a call target, dispatching
                // through the same method-table path as named functions
                // (Issue #10933). If no method exists, the runtime raises
                // UndefVarError/MethodError like any other call.
                Ok(ResolvedCallTarget::DirectCall { name: op_text })
            }
        }
        _ => Err(UnsupportedFeature::new(
            UnsupportedFeatureKind::UnsupportedCallTarget,
            walker.span(&callee),
        )),
    }
}

/// Build a LetBlock for indirect calls (field or index expression call targets).
fn build_indirect_call(
    temp_name: String,
    callee_expr: Expr,
    args: Vec<Expr>,
    kwargs: Vec<(crate::ir::core::InternedStr, Expr)>,
    splat_mask: Vec<bool>,
    kwargs_splat_mask: Vec<bool>,
    span: crate::span::Span,
) -> Expr {
    let call_expr = Expr::Call {
        function: temp_name.clone().into(),
        args,
        kwargs,
        splat_mask,
        kwargs_splat_mask,
        span,
    };

    Expr::LetBlock {
        bindings: vec![(temp_name.into(), callee_expr)],
        body: Block {
            stmts: vec![Stmt::Expr {
                expr: call_expr,
                span,
            }],
            span,
        },
        span,
    }
}

/// Extract the include path string from args for error messages.
fn extract_include_path<'a>(walker: &CstWalker<'a>, args_node: Option<Node<'a>>) -> String {
    if let Some(arg_node) = args_node {
        if let Some(first_arg) = walker.named_children(&arg_node).next() {
            if walker.kind(&first_arg) == NodeKind::StringLiteral {
                let text = walker.text(&first_arg);
                return text.trim_matches('"').to_string();
            } else {
                return "<dynamic path>".to_string();
            }
        }
        return "<unknown>".to_string();
    }
    "<missing argument>".to_string()
}

/// Find the ArgumentList or TupleExpression node in named children (skipping the callee).
fn find_args_node<'a>(walker: &CstWalker<'a>, named: &[Node<'a>]) -> Option<Node<'a>> {
    named
        .iter()
        .skip(1)
        .find(|n| {
            matches!(
                walker.kind(n),
                NodeKind::ArgumentList | NodeKind::TupleExpression
            )
        })
        .copied()
}

/// Lower arrow function expression: x -> expr or (x, y) -> expr
pub fn lower_arrow_function<'a>(
    walker: &CstWalker<'a>,
    node: Node<'a>,
    lambda_ctx: &LambdaContext,
) -> LowerResult<Expr> {
    let span = walker.span(&node);
    let children = walker.named_children_vec(&node);

    let mut params: Vec<TypedParam> = Vec::new();
    let mut kwparams: Vec<KwParam> = Vec::new();
    // Optional positional default values, index-aligned with `params` (Issue #8047).
    // `(x, d=2) -> ...` must bind the default in the reduced-arity call, mirroring
    // the named/short/block forms that go through `generate_default_arg_stubs`.
    let mut defaults: Vec<Option<Expr>> = Vec::new();
    let mut body_expr: Option<Expr> = None;
    let child_count = children.len();

    for (i, child) in children.iter().enumerate() {
        let is_last = i == child_count - 1;
        if is_last {
            let requires_value_context =
                crate::lowering::contains_value_parametric_base(walker, *child, &params, &kwparams);
            body_expr = Some(
                if requires_value_context || lambda_ctx.has_active_type_params() {
                    lambda_ctx.with_function_body_scope(|| {
                        lambda_ctx.with_active_value_params(&params, &kwparams, || {
                            lower_expr_with_ctx(walker, *child, lambda_ctx)
                        })
                    })?
                } else {
                    lower_expr(walker, *child)?
                },
            );
        } else {
            match walker.kind(child) {
                NodeKind::Identifier => {
                    let name = walker.text(child).to_string();
                    params.push(TypedParam::untyped(name, walker.span(child)));
                    defaults.push(None);
                }
                NodeKind::ArgumentList
                | NodeKind::TupleExpression
                | NodeKind::ParenthesizedExpression
                | NodeKind::ParameterList => {
                    collect_lifted_arrow_parameters(
                        walker,
                        *child,
                        &mut params,
                        &mut kwparams,
                        &mut defaults,
                    )?;
                }
                _ => {}
            }
        }
    }

    let body_expr = body_expr.ok_or_else(|| {
        UnsupportedFeature::new(
            UnsupportedFeatureKind::UnsupportedExpression("lambda without body".to_string()),
            span,
        )
    })?;

    let lambda_name = lambda_ctx.next_lambda_name();
    let body = Block {
        stmts: vec![Stmt::Return {
            value: Some(body_expr),
            span,
        }],
        span,
    };
    let (params, body) = inject_parameter_destructuring_prologue(params, body);
    let func = Function {
        name: lambda_name.clone(),
        params,
        kwparams,
        type_params: Vec::new(),
        return_type: None,
        body,
        is_base_extension: false,
        is_runtime_eval: false,
        span,
        new_struct_name: None,
    }
    .into_lowering_helper();

    let func = inject_kwarg_default_prologues(func);

    // Emit reduced-arity stub methods so optional positional defaults bind in
    // shorter calls, e.g. `(x, d=2) -> ...` called as `a(1)` (Issue #8047).
    // The stubs share `lambda_name`, forming a method table dispatched by arity,
    // exactly as the named/short/block default-arg paths do.
    let stubs = generate_default_arg_stubs(&func, &defaults);
    lambda_ctx.add_lifted_function(func);
    for stub in stubs {
        lambda_ctx.add_lifted_function(stub);
    }

    Ok(Expr::FunctionRef {
        name: lambda_name.into(),
        span,
    })
}

fn collect_lifted_arrow_parameters<'a>(
    walker: &CstWalker<'a>,
    node: Node<'a>,
    params: &mut Vec<TypedParam>,
    kwparams: &mut Vec<KwParam>,
    defaults: &mut Vec<Option<Expr>>,
) -> LowerResult<()> {
    let mut in_kwargs = false;
    for arg in walker.named_children(&node) {
        match walker.kind(&arg) {
            NodeKind::Semicolon => in_kwargs = true,
            NodeKind::Identifier if !in_kwargs => {
                let name = walker.text(&arg).to_string();
                params.push(TypedParam::untyped(name, walker.span(&arg)));
                defaults.push(None);
            }
            // Optional positional default `d=2` (and typed `d::Int=2`) parses as
            // an `Assignment` node inside the arrow parameter tuple (Issue #8047).
            // The LHS is the parameter (untyped Identifier or a typed parameter),
            // the RHS is the default expression.
            NodeKind::Assignment if !in_kwargs => {
                let assign_children: Vec<_> = walker
                    .named_children(&arg)
                    .filter(|n| walker.kind(n) != NodeKind::Operator)
                    .collect();
                if assign_children.len() >= 2 {
                    let lhs = assign_children[0];
                    let rhs = assign_children[assign_children.len() - 1];
                    params.push(parse_parameter(walker, lhs)?);
                    defaults.push(Some(lower_expr(walker, rhs)?));
                }
            }
            NodeKind::Parameter | NodeKind::TypedExpression | NodeKind::TupleExpression
                if !in_kwargs =>
            {
                params.push(parse_parameter(walker, arg)?);
                defaults.push(None);
            }
            NodeKind::SplatExpression | NodeKind::SplatParameter if !in_kwargs => {
                params.push(parse_parameter(walker, arg)?);
                defaults.push(None);
            }
            // Every post-`;` node is a keyword parameter, routed through the SAME
            // authority the named-function signature path uses (Issue #10354).
            // This replaces the local `lower_arrow_kwparam` copy, which recognized
            // only the bare-Identifier `KwParameter` shape (so an ANNOTATED keyword
            // — an `Assignment` with a `TypedExpression` LHS — fell into the
            // `_ => {}` below and was silently DROPPED from the lambda's signature)
            // AND, even on the shapes it did match, hard-coded the declared type to
            // `None`, discarding the `::T` annotation that Issue #11024 added to
            // `KwParam` precisely so it could be enforced. See
            // `lowering::function::signature::parse_kwparam_node`.
            _ if in_kwargs => {
                if let Some(kw) =
                    crate::lowering::function::signature::parse_kwparam_node(walker, arg)?
                {
                    kwparams.push(kw);
                }
            }
            _ => {}
        }
    }
    Ok(())
}

/// Lower call expression with lambda context (handles do syntax)
pub fn lower_call_expr_with_ctx<'a>(
    walker: &CstWalker<'a>,
    node: Node<'a>,
    lambda_ctx: &LambdaContext,
) -> LowerResult<Expr> {
    let span = walker.span(&node);
    let named = walker.named_children_vec(&node);

    if named.is_empty() {
        return Err(UnsupportedFeature::new(
            UnsupportedFeatureKind::UnsupportedCallTarget,
            span,
        ));
    }

    let callee = named[0];

    // Find ArgumentList and check for do_clause inside it
    let args_node = find_args_node(walker, &named);

    // Check for do_clause inside ArgumentList (Pure Rust parser puts it there)
    let do_clause = args_node.and_then(|args| {
        walker
            .named_children(&args)
            .find(|n| walker.kind(n) == NodeKind::DoClause)
    });

    // Check for operator partial application: ==(x), >(3), (!=)(val), etc. (Issue #3119)
    // Function bodies need nested closures so captures are discovered as free variables.
    if let Some(op) = extract_partial_apply_operator(walker, callee) {
        let (args, _, _, _) = match args_node {
            Some(node) => lower_argument_list_with_ctx(walker, node, lambda_ctx)?,
            None => (Vec::new(), Vec::new(), Vec::new(), Vec::new()),
        };
        if args.len() == 1 {
            let arg = args.into_iter().next().ok_or_else(|| {
                internal_lowering_error(span, "operator partial-apply argument count checked above")
            })?;
            if lambda_ctx.prefer_nested_lambdas() {
                return lower_operator_partial_apply_as_nested(&op, arg, span);
            }
            return lower_operator_partial_apply(&op, arg, lambda_ctx, span);
        }
    }

    // Handle immediately invoked lambda: (x -> expr)(args) (Issue #3142)
    // Function-body lowering embeds the lambda so capture analysis sees outer locals.
    if let Some(arrow_node) = extract_paren_arrow_function(walker, callee) {
        let lambda_ref = if lambda_ctx.prefer_nested_lambdas() {
            lower_arrow_value_as_nested_with_ctx(walker, arrow_node, lambda_ctx)?
        } else {
            lower_arrow_function(walker, arrow_node, lambda_ctx)?
        };
        let temp_name = format!("__iife_{}", span.start);
        let (args, kwargs, splat_mask, kwargs_splat_mask) = match args_node {
            Some(node) => lower_argument_list_with_ctx(walker, node, lambda_ctx)?,
            None => (Vec::new(), Vec::new(), Vec::new(), Vec::new()),
        };
        return Ok(build_indirect_call(
            temp_name,
            lambda_ref,
            args,
            kwargs,
            splat_mask,
            kwargs_splat_mask,
            span,
        ));
    }

    if walker.kind(&callee) == NodeKind::ParenthesizedExpression {
        let children = walker.named_children_vec(&callee);
        if children.len() == 1 {
            if walker.kind(&children[0]) == NodeKind::FunctionDefinition {
                let (args, kwargs, splat_mask, kwargs_splat_mask) = match args_node {
                    Some(node) => lower_argument_list_with_ctx(walker, node, lambda_ctx)?,
                    None => (Vec::new(), Vec::new(), Vec::new(), Vec::new()),
                };
                return lower_fullform_iife_as_nested(
                    walker,
                    children[0],
                    Some(lambda_ctx),
                    args,
                    kwargs,
                    splat_mask,
                    kwargs_splat_mask,
                    span,
                );
            }
            let callee_expr = lower_expr_with_ctx(walker, children[0], lambda_ctx)?;
            let temp_name = format!("__paren_func_{}", span.start);
            let (args, kwargs, splat_mask, kwargs_splat_mask) = match args_node {
                Some(node) => lower_argument_list_with_ctx(walker, node, lambda_ctx)?,
                None => (Vec::new(), Vec::new(), Vec::new(), Vec::new()),
            };
            return Ok(build_indirect_call(
                temp_name,
                callee_expr,
                args,
                kwargs,
                splat_mask,
                kwargs_splat_mask,
                span,
            ));
        }
    }

    // Call result as callee: `make_adder(a)(x)`. Julia treats the result of the
    // first call as a callable object; lower it through the same indirect-call
    // path used for field and indexed callable values.
    if walker.kind(&callee) == NodeKind::CallExpression {
        let callee_expr = lower_expr_with_ctx(walker, callee, lambda_ctx)?;
        let temp_name = format!("__call_result_func_{}", span.start);
        let (args, kwargs, splat_mask, kwargs_splat_mask) = match args_node {
            Some(node) => lower_argument_list_with_ctx(walker, node, lambda_ctx)?,
            None => (Vec::new(), Vec::new(), Vec::new(), Vec::new()),
        };
        return Ok(build_indirect_call(
            temp_name,
            callee_expr,
            args,
            kwargs,
            splat_mask,
            kwargs_splat_mask,
            span,
        ));
    }

    // Resolve call target using shared helper (Issue #2271)
    match resolve_call_target(walker, callee, &named, span)? {
        ResolvedCallTarget::ModuleCall { module, function } => {
            if let Some(do_node) = do_clause {
                let (regular_args, kwargs, splat_mask, kwargs_splat_mask) = match args_node {
                    Some(node) => lower_argument_list_with_ctx(walker, node, lambda_ctx)?,
                    None => (Vec::new(), Vec::new(), Vec::new(), Vec::new()),
                };
                if lambda_ctx.prefer_nested_lambdas() {
                    return lower_do_module_call_as_nested(
                        walker,
                        do_node,
                        module,
                        function,
                        regular_args,
                        splat_mask,
                        kwargs,
                        kwargs_splat_mask,
                        span,
                        Some(lambda_ctx),
                    );
                }
                let lambda_ref = lower_do_clause(walker, do_node, lambda_ctx)?;
                let mut all_args = vec![lambda_ref];
                all_args.extend(regular_args);
                let mut all_splat_mask = vec![false];
                all_splat_mask.extend(splat_mask);
                return Ok(Expr::ModuleCall {
                    module: module.into(),
                    function: function.into(),
                    args: all_args,
                    kwargs,
                    splat_mask: all_splat_mask,
                    kwargs_splat_mask,
                    span,
                });
            }
            let (args, kwargs, splat_mask, kwargs_splat_mask) = match args_node {
                Some(node) => lower_argument_list_with_ctx(walker, node, lambda_ctx)?,
                None => (Vec::new(), Vec::new(), Vec::new(), Vec::new()),
            };
            Ok(Expr::ModuleCall {
                module: module.into(),
                function: function.into(),
                args,
                kwargs,
                splat_mask,
                kwargs_splat_mask,
                span,
            })
        }
        ResolvedCallTarget::IndirectCall { expr, temp_name } => {
            let (args, kwargs, splat_mask, kwargs_splat_mask) = match args_node {
                Some(node) => lower_argument_list_with_ctx(walker, node, lambda_ctx)?,
                None => (Vec::new(), Vec::new(), Vec::new(), Vec::new()),
            };
            Ok(build_indirect_call(
                temp_name,
                expr,
                args,
                kwargs,
                splat_mask,
                kwargs_splat_mask,
                span,
            ))
        }
        ResolvedCallTarget::UnaryNot { operand } => Ok(Expr::UnaryOp {
            op: UnaryOp::Not,
            operand: Box::new(operand),
            span,
        }),
        ResolvedCallTarget::DirectCall { name } => {
            // Handle do clause: map([1,2,3]) do x; x^2 end
            if let Some(do_node) = do_clause {
                // Get regular arguments (excluding do_clause)
                let (regular_args, kwargs, splat_mask, kwargs_splat_mask) = match args_node {
                    Some(node) => lower_argument_list_with_ctx(walker, node, lambda_ctx)?,
                    None => (Vec::new(), Vec::new(), Vec::new(), Vec::new()),
                };
                if lambda_ctx.prefer_nested_lambdas() {
                    return lower_do_call_as_nested(
                        walker,
                        do_node,
                        name,
                        regular_args,
                        splat_mask,
                        kwargs,
                        kwargs_splat_mask,
                        span,
                        Some(lambda_ctx),
                    );
                }
                let lambda_ref = lower_do_clause(walker, do_node, lambda_ctx)?;

                // For do syntax: function(lambda_ref, regular_args...)
                let mut all_args = vec![lambda_ref];
                all_args.extend(regular_args);
                let mut all_splat_mask = vec![false];
                all_splat_mask.extend(splat_mask);

                return Ok(Expr::Call {
                    function: name.into(),
                    args: all_args,
                    kwargs,
                    splat_mask: all_splat_mask,
                    kwargs_splat_mask,
                    span,
                });
            }

            // Regular call - check if any argument is an arrow function
            let (args, kwargs, splat_mask, kwargs_splat_mask) = match args_node {
                Some(arg_node) => lower_argument_list_with_ctx(walker, arg_node, lambda_ctx)?,
                None => (Vec::new(), Vec::new(), Vec::new(), Vec::new()),
            };

            // Privileged inner-constructor `new` rejects keywords upstream.
            // A keyword-bearing call with this spelling is therefore an
            // ordinary binding lookup; preserve its values and splat masks
            // instead of collapsing it into compact Expr::New IR (#11204).
            if is_new_call_name(&name)
                && (!kwargs.is_empty() || kwargs_splat_mask.iter().any(|value| *value))
            {
                let callee_expr = if name == "new" {
                    Expr::Var("new".into(), span)
                } else {
                    lower_expr_with_ctx(walker, callee, lambda_ctx)?
                };
                return Ok(build_indirect_call(
                    format!("__keyword_new_callable_{}", span.start),
                    callee_expr,
                    args,
                    kwargs,
                    splat_mask,
                    kwargs_splat_mask,
                    span,
                ));
            }

            if let Some(new_expr) = lower_new_call_from_args(&name, args.clone(), &splat_mask, span)
            {
                return Ok(new_expr);
            }

            // Special handling for range(start, stop; length=N) -> range(start, stop, N)
            if name == "range" && args.len() == 2 && !kwargs.is_empty() {
                for (key, value) in &kwargs {
                    if key == "length" {
                        let mut positional_args = args.clone();
                        positional_args.push(value.clone());
                        return Ok(Expr::Call {
                            function: name.into(),
                            args: positional_args,
                            kwargs: vec![],
                            splat_mask: vec![],
                            kwargs_splat_mask: vec![],
                            span,
                        });
                    }
                }
            }

            // Broadcast operator function call syntax: .+(a, b), .*(a, b) etc. (Issue #2685)
            // Convert to materialize(Broadcasted(op, (args...))) pipeline
            if is_broadcast_op(&name) {
                let base_op = super::strip_broadcast_dot(&name);
                let fn_name = match base_op {
                    "&&" => "andand",
                    "||" => "oror",
                    other => other,
                };
                return Ok(super::make_broadcasted_call(fn_name, args, span));
            }

            if let Some(builtin) = map_builtin_name(&name) {
                let args = if builtin == BuiltinOp::ExprNew
                    && splat_mask.iter().any(|is_splat| *is_splat)
                {
                    wrap_expr_splat_args(args, splat_mask, span)
                } else {
                    args
                };
                return Ok(Expr::Builtin {
                    name: builtin,
                    args,
                    span,
                });
            }

            Ok(Expr::Call {
                function: name.into(),
                args,
                kwargs,
                splat_mask,
                kwargs_splat_mask,
                span,
            })
        }
    }
}

/// Extract the do-clause lambda's parameters and body block (shared by both lowering paths).
fn extract_do_clause_parts<'a>(
    walker: &CstWalker<'a>,
    node: Node<'a>,
    lambda_ctx: Option<&LambdaContext>,
) -> LowerResult<(Vec<TypedParam>, Block)> {
    let span = walker.span(&node);
    let children = walker.named_children_vec(&node);

    // Structure: do, ParameterList/identifier(s), block, end
    // Pure Rust parser wraps params in ParameterList, tree-sitter uses direct identifiers
    let mut params: Vec<TypedParam> = Vec::new();
    let mut body_block: Option<Block> = None;

    for child in children {
        match walker.kind(&child) {
            NodeKind::ParameterList => {
                // Pure Rust parser: parameters wrapped in ParameterList. A
                // single tuple child is `do (a, b)` destructuring and must stay
                // one parameter; `do a, b` remains multiple parameters.
                let param_nodes = walker.named_children_vec(&child);
                if param_nodes.len() == 1
                    && walker.kind(&param_nodes[0]) == NodeKind::TupleExpression
                {
                    params.push(parse_parameter(walker, param_nodes[0])?);
                } else {
                    for param in param_nodes {
                        params.push(parse_parameter(walker, param)?);
                    }
                }
            }
            NodeKind::Identifier
            | NodeKind::TupleExpression
            | NodeKind::Parameter
            | NodeKind::TypedExpression
            | NodeKind::SplatExpression
            | NodeKind::SplatParameter => {
                // Tree-sitter style: direct identifiers
                params.push(parse_parameter(walker, child)?);
            }
            NodeKind::Block => {
                body_block = Some(lower_block_simple(walker, child, lambda_ctx)?);
            }
            _ => {}
        }
    }

    let body = body_block.unwrap_or(Block {
        stmts: vec![],
        span,
    });

    Ok(inject_parameter_destructuring_prologue(params, body))
}

/// Lower a do-clause trailing closure inside a full-form function body (no `LambdaContext`).
///
/// Mirrors `lower_arrow_value_as_nested` (Issue #4289): the no-context lowering path cannot
/// add to the top-level `LambdaContext`, so the do-block lambda is embedded as a nested
/// `FunctionDef` inside a `LetBlock` and the enclosing call is desugared as
/// `f(__do_block_N, regular_args...)` (closure prepended as the FIRST argument, matching
/// upstream Julia's `f(args...) do x; body end` ≡ `f(x -> body, args...)`).
///
/// Without this, do-blocks attached to calls inside `function ... end` / `f() = ...` bodies
/// were silently dropped, because the no-context `lower_call_expr` never inspected the
/// `DoClause` node (Issue #5227). The produced IR lets `collect_block_functions` discover the
/// lambda as a nested function of the enclosing function, enabling closure capture of outer
/// variables (e.g. `n` in `get!(cache, n) do; n * n end`).
///
/// Produced IR:
/// ```text
/// LetBlock {
///   bindings: [],
///   body: [
///     Stmt::FunctionDef(__do_block_N),                 // defines the do-block lambda
///     Stmt::Expr(Call(name, [Var(__do_block_N), regular_args...])),  // the desugared call
///   ]
/// }
/// ```
fn lower_do_call_as_nested<'a>(
    walker: &CstWalker<'a>,
    do_node: Node<'a>,
    name: String,
    regular_args: Vec<Expr>,
    regular_splat_mask: Vec<bool>,
    kwargs: Vec<(crate::ir::core::InternedStr, Expr)>,
    kwargs_splat_mask: Vec<bool>,
    span: crate::span::Span,
    lambda_ctx: Option<&LambdaContext>,
) -> LowerResult<Expr> {
    let do_span = walker.span(&do_node);
    let (params, body) = extract_do_clause_parts(walker, do_node, lambda_ctx)?;

    let lambda_name = lambda_ctx.map_or_else(
        || format!("__do_block_{}", do_span.start),
        |context| context.do_block_name(do_span.start),
    );
    let func = Function {
        name: lambda_name.clone(),
        params,
        kwparams: vec![],
        type_params: Vec::new(),
        return_type: None,
        body,
        is_base_extension: false,
        is_runtime_eval: false,
        span: do_span,
        new_struct_name: None,
    }
    .into_lowering_helper();

    // For do syntax: function(lambda_ref, regular_args...) — closure is the FIRST argument.
    let mut all_args = vec![Expr::Var(lambda_name.clone().into(), do_span)];
    all_args.extend(regular_args);
    let mut splat_mask = vec![false];
    splat_mask.extend(regular_splat_mask);

    let call_expr = Expr::Call {
        function: name.into(),
        args: all_args,
        kwargs,
        splat_mask,
        kwargs_splat_mask,
        span,
    };

    Ok(Expr::LetBlock {
        bindings: vec![],
        body: Block {
            stmts: vec![
                Stmt::FunctionDef {
                    func: Box::new(func),
                    span: do_span,
                },
                Stmt::Expr {
                    expr: call_expr,
                    span,
                },
            ],
            span,
        },
        span,
    })
}

fn lower_do_module_call_as_nested<'a>(
    walker: &CstWalker<'a>,
    do_node: Node<'a>,
    module: String,
    function: String,
    regular_args: Vec<Expr>,
    regular_splat_mask: Vec<bool>,
    kwargs: Vec<(crate::ir::core::InternedStr, Expr)>,
    kwargs_splat_mask: Vec<bool>,
    span: crate::span::Span,
    lambda_ctx: Option<&LambdaContext>,
) -> LowerResult<Expr> {
    let do_span = walker.span(&do_node);
    let (params, body) = extract_do_clause_parts(walker, do_node, lambda_ctx)?;

    let lambda_name = lambda_ctx.map_or_else(
        || format!("__do_block_{}", do_span.start),
        |context| context.do_block_name(do_span.start),
    );
    let func = Function {
        name: lambda_name.clone(),
        params,
        kwparams: vec![],
        type_params: Vec::new(),
        return_type: None,
        body,
        is_base_extension: false,
        is_runtime_eval: false,
        span: do_span,
        new_struct_name: None,
    }
    .into_lowering_helper();

    let mut all_args = vec![Expr::Var(lambda_name.clone().into(), do_span)];
    all_args.extend(regular_args);
    let mut splat_mask = vec![false];
    splat_mask.extend(regular_splat_mask);

    let call_expr = Expr::ModuleCall {
        module: module.into(),
        function: function.into(),
        args: all_args,
        kwargs,
        splat_mask,
        kwargs_splat_mask,
        span,
    };

    Ok(Expr::LetBlock {
        bindings: vec![],
        body: Block {
            stmts: vec![
                Stmt::FunctionDef {
                    func: Box::new(func),
                    span: do_span,
                },
                Stmt::Expr {
                    expr: call_expr,
                    span,
                },
            ],
            span,
        },
        span,
    })
}

/// Lower do clause to a FunctionRef
fn lower_do_clause<'a>(
    walker: &CstWalker<'a>,
    node: Node<'a>,
    lambda_ctx: &LambdaContext,
) -> LowerResult<Expr> {
    let span = walker.span(&node);
    let (params, body) = extract_do_clause_parts(walker, node, Some(lambda_ctx))?;

    // Create a function
    let lambda_name = lambda_ctx.next_lambda_name();
    let func = Function {
        name: lambda_name.clone(),
        params,
        kwparams: vec![],
        type_params: Vec::new(),
        return_type: None,
        body,
        is_base_extension: false,
        is_runtime_eval: false,
        span,
        new_struct_name: None,
    };

    lambda_ctx.add_lifted_function(func);

    Ok(Expr::FunctionRef {
        name: lambda_name.into(),
        span,
    })
}

/// Simple block lowering without lambda context (for do blocks)
fn lower_block_simple<'a>(
    walker: &CstWalker<'a>,
    node: Node<'a>,
    lambda_ctx: Option<&LambdaContext>,
) -> LowerResult<Block> {
    let span = walker.span(&node);
    let mut stmts = Vec::new();

    let children: Vec<_> = walker.named_children_vec(&node);
    let last_id = children.last().map(|n| n.id());

    for child in children {
        // For do blocks, we need to wrap the last expression as a return
        let is_last = Some(child.id()) == last_id;
        let child_span = walker.span(&child);

        match walker.kind(&child) {
            NodeKind::ReturnStatement => {
                let value = walker
                    .named_children_vec(&child)
                    .pop()
                    .map(|n| match lambda_ctx {
                        Some(ctx) => lower_expr_with_ctx(walker, n, ctx),
                        None => lower_expr(walker, n),
                    })
                    .transpose()?;
                stmts.push(Stmt::Return {
                    value,
                    span: child_span,
                });
            }
            NodeKind::Assignment | NodeKind::BinaryExpression
                if walker.kind(&child) == NodeKind::Assignment
                    || is_binary_assignment_expr(walker, child) =>
            {
                // Handle assignment statements properly
                let stmt = match lambda_ctx {
                    Some(ctx) => lower_stmt_with_ctx(walker, child, ctx)?,
                    None => lower_stmt(walker, child)?,
                };
                if is_last {
                    // If the last statement is an assignment, we need to return the assigned value
                    // Extract the variable name from the assignment and return it
                    if let Stmt::Assign { var, .. } = &stmt {
                        let var_name = var.clone();
                        stmts.push(stmt);
                        stmts.push(Stmt::Return {
                            value: Some(Expr::Var(var_name.into(), child_span)),
                            span: child_span,
                        });
                    } else {
                        stmts.push(stmt);
                    }
                } else {
                    stmts.push(stmt);
                }
            }
            NodeKind::MacroCall => {
                let stmt = match lambda_ctx {
                    Some(ctx) => lower_stmt_with_ctx(walker, child, ctx)?,
                    None => lower_stmt(walker, child)?,
                };
                if is_last {
                    match &stmt {
                        Stmt::Expr { expr, .. } => {
                            stmts.push(Stmt::Return {
                                value: Some(expr.clone()),
                                span: child_span,
                            });
                        }
                        Stmt::Assign { var, .. } => {
                            let var_name = var.clone();
                            stmts.push(stmt);
                            stmts.push(Stmt::Return {
                                value: Some(Expr::Var(var_name.into(), child_span)),
                                span: child_span,
                            });
                        }
                        _ => stmts.push(stmt),
                    }
                } else {
                    stmts.push(stmt);
                }
            }
            // Statement-only constructs in a do-block body lower through the
            // general statement path (Issue #10353) — previously `for`/`while`
            // fell into the expression path below and failed with
            // `UnsupportedExpression("for_statement")`. As the LAST child they
            // evaluate to `nothing`, matching upstream do-block semantics.
            NodeKind::ForStatement
            | NodeKind::WhileStatement
            | NodeKind::GlobalStatement
            | NodeKind::LocalStatement
            | NodeKind::ConstStatement => {
                let stmt = match lambda_ctx {
                    Some(ctx) => lower_stmt_with_ctx(walker, child, ctx)?,
                    None => lower_stmt(walker, child)?,
                };
                stmts.push(stmt);
                if is_last {
                    stmts.push(Stmt::Return {
                        value: None,
                        span: child_span,
                    });
                }
            }
            _ => {
                let expr = match lambda_ctx {
                    Some(ctx) => lower_expr_with_ctx(walker, child, ctx)?,
                    None => lower_expr(walker, child)?,
                };
                if is_last {
                    // Last expression becomes implicit return
                    stmts.push(Stmt::Return {
                        value: Some(expr),
                        span: child_span,
                    });
                } else {
                    stmts.push(Stmt::Expr {
                        expr,
                        span: child_span,
                    });
                }
            }
        }
    }

    Ok(Block { stmts, span })
}

fn is_binary_assignment_expr<'a>(walker: &CstWalker<'a>, node: Node<'a>) -> bool {
    walker.kind(&node) == NodeKind::BinaryExpression
        && walker
            .children(&node)
            .iter()
            .any(|child| child.kind() == "operator" && walker.text(child) == "=")
}

/// Lower argument list with lambda context (handles arrow functions as arguments)
/// Returns (positional_args, keyword_args, splat_mask, kwargs_splat_mask)
pub(in crate::lowering::expr) fn lower_argument_list_with_ctx<'a>(
    walker: &CstWalker<'a>,
    node: Node<'a>,
    lambda_ctx: &LambdaContext,
) -> LowerResult<(
    Vec<Expr>,
    Vec<(crate::ir::core::InternedStr, Expr)>,
    Vec<bool>,
    Vec<bool>,
)> {
    let children = walker.named_children_vec(&node);

    let mut positional_args = Vec::new();
    let mut kwargs = Vec::new();
    let mut splat_mask = Vec::new();
    let mut kwargs_splat_mask = Vec::new();
    let mut saw_semicolon = false;

    for child in children {
        let kind = walker.kind(&child);

        // Check for semicolon node (marks transition to kwargs)
        if kind == NodeKind::Semicolon {
            saw_semicolon = true;
            continue;
        }

        match kind {
            NodeKind::ArrowFunctionExpression => {
                let expr = if lambda_ctx.prefer_nested_lambdas() {
                    lower_arrow_value_as_nested_with_ctx(walker, child, lambda_ctx)?
                } else {
                    lower_arrow_function(walker, child, lambda_ctx)?
                };
                if saw_semicolon {
                    // After semicolon - this would be a kwarg, but arrow functions as kwargs are unusual
                    positional_args.push(expr);
                    splat_mask.push(false);
                } else {
                    positional_args.push(expr);
                    splat_mask.push(false);
                }
            }
            NodeKind::DoClause => {
                // Skip DoClause (handled separately by caller)
            }
            NodeKind::SplatExpression if saw_semicolon => {
                // Kwargs splat expression after semicolon: f(; opts...)
                // Extract inner expression and mark for runtime kwargs expansion
                if let Some(inner) = walker.named_children(&child).next() {
                    // Use empty string as key to mark this as a splat
                    kwargs.push((
                        "".to_string().into(),
                        lower_expr_with_ctx(walker, inner, lambda_ctx)?,
                    ));
                    kwargs_splat_mask.push(true);
                }
            }
            NodeKind::SplatExpression => {
                // Positional splat expression: args... - extract inner expression and mark for runtime expansion
                if let Some(inner) = walker.named_children(&child).next() {
                    positional_args.push(lower_expr_with_ctx(walker, inner, lambda_ctx)?);
                    splat_mask.push(true);
                }
            }
            NodeKind::Assignment if saw_semicolon => {
                // Keyword argument after semicolon
                if let Some((name, value)) =
                    parse_kwarg_assignment(walker, child, Some(lambda_ctx))?
                {
                    kwargs.push((name, value));
                    kwargs_splat_mask.push(false);
                }
            }
            NodeKind::KeywordArgument if saw_semicolon => {
                // Pure Rust parser: KeywordArgument after semicolon
                if let Some((name, value)) =
                    parse_keyword_argument(walker, child, Some(lambda_ctx))?
                {
                    kwargs.push((name, value));
                    kwargs_splat_mask.push(false);
                }
            }
            NodeKind::Assignment => {
                // Before semicolon - could be keyword argument without semicolon
                if let Some((name, value)) =
                    parse_kwarg_assignment(walker, child, Some(lambda_ctx))?
                {
                    kwargs.push((name, value));
                    kwargs_splat_mask.push(false);
                } else {
                    positional_args.push(lower_expr_with_ctx(walker, child, lambda_ctx)?);
                    splat_mask.push(false);
                }
            }
            NodeKind::KeywordArgument => {
                // Pure Rust parser: KeywordArgument before semicolon
                if let Some((name, value)) =
                    parse_keyword_argument(walker, child, Some(lambda_ctx))?
                {
                    kwargs.push((name, value));
                    kwargs_splat_mask.push(false);
                }
            }
            NodeKind::Operator => {
                // Bare operator as function argument: map(+, ...), reduce(*, ...) (Issue #1985)
                let op_text = walker.text(&child).to_string();
                let op_span = walker.span(&child);
                positional_args.push(Expr::FunctionRef {
                    name: op_text.into(),
                    span: op_span,
                });
                splat_mask.push(false);
            }
            _ => {
                positional_args.push(lower_expr_with_ctx(walker, child, lambda_ctx)?);
                splat_mask.push(false);
            }
        }
    }

    Ok((positional_args, kwargs, splat_mask, kwargs_splat_mask))
}

/// Lower call expression without lambda context
pub fn lower_call_expr<'a>(walker: &CstWalker<'a>, node: Node<'a>) -> LowerResult<Expr> {
    let span = walker.span(&node);
    let named = walker.named_children_vec(&node);
    if named.is_empty() {
        return Err(UnsupportedFeature::new(
            UnsupportedFeatureKind::UnsupportedCallTarget,
            span,
        ));
    }

    let callee = named[0];

    // Check for operator partial application: ==(x), >(3), (!=)(val), etc. (Issue #3119)
    // Use the LetBlock+FunctionDef variant so that the generated lambda is embedded as a nested
    // function definition, allowing collect_stmt_functions to discover it and enabling correct
    // closure capture analysis when this path is taken inside a full-form function body.
    if let Some(op) = extract_partial_apply_operator(walker, callee) {
        let args_node = find_args_node(walker, &named);
        let (args, _, _, _) = match args_node {
            Some(node) => lower_argument_list_with_kwargs(walker, node)?,
            None => (Vec::new(), Vec::new(), Vec::new(), Vec::new()),
        };
        if args.len() == 1 {
            let arg = args.into_iter().next().ok_or_else(|| {
                internal_lowering_error(span, "operator partial-apply argument count checked above")
            })?;
            return lower_operator_partial_apply_as_nested(&op, arg, span);
        }
    }

    // Handle immediately invoked lambda: (x -> expr)(args) (Issue #3142)
    // Embed the lambda as a nested FunctionDef inside a LetBlock and call it immediately.
    if let Some(arrow_node) = extract_paren_arrow_function(walker, callee) {
        let args_node = find_args_node(walker, &named);
        let (args, kwargs, splat_mask, kwargs_splat_mask) = match args_node {
            Some(node) => lower_argument_list_with_kwargs(walker, node)?,
            None => (Vec::new(), Vec::new(), Vec::new(), Vec::new()),
        };
        return lower_iife_as_nested(
            walker,
            arrow_node,
            args,
            kwargs,
            splat_mask,
            kwargs_splat_mask,
            span,
        );
    }

    if walker.kind(&callee) == NodeKind::ParenthesizedExpression {
        let children = walker.named_children_vec(&callee);
        if children.len() == 1 {
            // Immediately-invoked full-form anonymous function:
            // `(function(x) x + 1 end)(arg)`. Lift the lambda into a nested
            // `FunctionDef` embedded in the `LetBlock` body (rather than routing
            // through `build_indirect_call`, which would bury it in a `LetBlock`
            // binding the nested-function collector never visits), so the lambda
            // is discovered/compiled as a nested function of the enclosing frame
            // and captures outer locals — matching the arrow IIFE path above
            // (Issue #8030).
            if walker.kind(&children[0]) == NodeKind::FunctionDefinition {
                let args_node = find_args_node(walker, &named);
                let (args, kwargs, splat_mask, kwargs_splat_mask) = match args_node {
                    Some(node) => lower_argument_list_with_kwargs(walker, node)?,
                    None => (Vec::new(), Vec::new(), Vec::new(), Vec::new()),
                };
                return lower_fullform_iife_as_nested(
                    walker,
                    children[0],
                    None,
                    args,
                    kwargs,
                    splat_mask,
                    kwargs_splat_mask,
                    span,
                );
            }
            let callee_expr = lower_expr(walker, children[0])?;
            let temp_name = format!("__paren_func_{}", span.start);
            let args_node = find_args_node(walker, &named);
            let (args, kwargs, splat_mask, kwargs_splat_mask) = match args_node {
                Some(node) => lower_argument_list_with_kwargs(walker, node)?,
                None => (Vec::new(), Vec::new(), Vec::new(), Vec::new()),
            };
            return Ok(build_indirect_call(
                temp_name,
                callee_expr,
                args,
                kwargs,
                splat_mask,
                kwargs_splat_mask,
                span,
            ));
        }
    }

    // Call result as callee: `make_adder(a)(x)`.
    if walker.kind(&callee) == NodeKind::CallExpression {
        let callee_expr = lower_expr(walker, callee)?;
        let temp_name = format!("__call_result_func_{}", span.start);
        let args_node = find_args_node(walker, &named);
        let (args, kwargs, splat_mask, kwargs_splat_mask) = match args_node {
            Some(node) => lower_argument_list_with_kwargs(walker, node)?,
            None => (Vec::new(), Vec::new(), Vec::new(), Vec::new()),
        };
        return Ok(build_indirect_call(
            temp_name,
            callee_expr,
            args,
            kwargs,
            splat_mask,
            kwargs_splat_mask,
            span,
        ));
    }

    // Resolve call target using shared helper (Issue #2271)
    match resolve_call_target(walker, callee, &named, span)? {
        ResolvedCallTarget::ModuleCall { module, function } => {
            let args_node = find_args_node(walker, &named);
            let do_clause = args_node.and_then(|args| {
                walker
                    .named_children(&args)
                    .find(|n| walker.kind(n) == NodeKind::DoClause)
            });
            if let Some(do_node) = do_clause {
                let (regular_args, kwargs, splat_mask, kwargs_splat_mask) = match args_node {
                    Some(node) => lower_argument_list_with_kwargs(walker, node)?,
                    None => (Vec::new(), Vec::new(), Vec::new(), Vec::new()),
                };
                return lower_do_module_call_as_nested(
                    walker,
                    do_node,
                    module,
                    function,
                    regular_args,
                    splat_mask,
                    kwargs,
                    kwargs_splat_mask,
                    span,
                    None,
                );
            }
            let (args, kwargs, splat_mask, kwargs_splat_mask) = match args_node {
                Some(node) => lower_argument_list_with_kwargs(walker, node)?,
                None => (Vec::new(), Vec::new(), Vec::new(), Vec::new()),
            };
            Ok(Expr::ModuleCall {
                module: module.into(),
                function: function.into(),
                args,
                kwargs,
                splat_mask,
                kwargs_splat_mask,
                span,
            })
        }
        ResolvedCallTarget::IndirectCall { expr, temp_name } => {
            let args_node = find_args_node(walker, &named);
            let (args, kwargs, splat_mask, kwargs_splat_mask) = match args_node {
                Some(node) => lower_argument_list_with_kwargs(walker, node)?,
                None => (Vec::new(), Vec::new(), Vec::new(), Vec::new()),
            };
            Ok(build_indirect_call(
                temp_name,
                expr,
                args,
                kwargs,
                splat_mask,
                kwargs_splat_mask,
                span,
            ))
        }
        ResolvedCallTarget::UnaryNot { operand } => Ok(Expr::UnaryOp {
            op: UnaryOp::Not,
            operand: Box::new(operand),
            span,
        }),
        ResolvedCallTarget::DirectCall { name } => {
            let args_node = find_args_node(walker, &named);

            // Handle do clause without a LambdaContext (inside a full-form function body):
            //   map(xs) do x; x*2 end  ≡  map(x -> x*2, xs)   (Issue #5227)
            // The no-context path cannot lift into the top-level LambdaContext, so embed the
            // do-block lambda as a nested FunctionDef in a LetBlock and prepend it as the
            // FIRST argument, matching upstream Julia's do-block desugaring.
            let do_clause = args_node.and_then(|args| {
                walker
                    .named_children(&args)
                    .find(|n| walker.kind(n) == NodeKind::DoClause)
            });
            if let Some(do_node) = do_clause {
                let (regular_args, kwargs, splat_mask, kwargs_splat_mask) = match args_node {
                    Some(node) => lower_argument_list_with_kwargs(walker, node)?,
                    None => (Vec::new(), Vec::new(), Vec::new(), Vec::new()),
                };
                return lower_do_call_as_nested(
                    walker,
                    do_node,
                    name,
                    regular_args,
                    splat_mask,
                    kwargs,
                    kwargs_splat_mask,
                    span,
                    None,
                );
            }

            // Special handling for include("path") - file inclusion
            if name == "include" {
                let path = extract_include_path(walker, args_node);
                return Err(UnsupportedFeature::new(
                    UnsupportedFeatureKind::IncludeCall(path),
                    span,
                ).with_hint("include is not supported in sandboxed environments. Use prelude for bundled functions, or define functions directly in the source."));
            }

            // Handle new() and new{T}() for inner constructors through the same
            // dedicated IR seam as the context-aware lowering lane.
            if is_new_call_name(&name) {
                let (args, kwargs, splat_mask, kwargs_splat_mask) = match args_node {
                    Some(node) => lower_argument_list_with_kwargs(walker, node)?,
                    None => (Vec::new(), Vec::new(), Vec::new(), Vec::new()),
                };
                if !kwargs.is_empty() || kwargs_splat_mask.iter().any(|value| *value) {
                    let callee_expr = if name == "new" {
                        Expr::Var("new".into(), span)
                    } else {
                        lower_expr(walker, callee)?
                    };
                    return Ok(build_indirect_call(
                        format!("__keyword_new_callable_{}", span.start),
                        callee_expr,
                        args,
                        kwargs,
                        splat_mask,
                        kwargs_splat_mask,
                        span,
                    ));
                }
                if let Some(new_expr) = lower_new_call_from_args(&name, args, &splat_mask, span) {
                    return Ok(new_expr);
                }
            }

            // Parse positional args and keyword args for non-new calls
            let (args, kwargs, splat_mask, kwargs_splat_mask) = match args_node {
                Some(node) => lower_argument_list_with_kwargs(walker, node)?,
                None => (Vec::new(), Vec::new(), Vec::new(), Vec::new()),
            };

            // Broadcast operator function call syntax: .+(a, b), .*(a, b) etc. (Issue #2685)
            // Convert to materialize(Broadcasted(op, (args...))) pipeline
            if is_broadcast_op(&name) {
                let base_op = super::strip_broadcast_dot(&name);
                let fn_name = match base_op {
                    "&&" => "andand",
                    "||" => "oror",
                    other => other,
                };
                return Ok(super::make_broadcasted_call(fn_name, args, span));
            }

            if let Some(builtin) = map_builtin_name(&name) {
                let args = if builtin == BuiltinOp::ExprNew
                    && splat_mask.iter().any(|is_splat| *is_splat)
                {
                    wrap_expr_splat_args(args, splat_mask, span)
                } else {
                    args
                };
                return Ok(Expr::Builtin {
                    name: builtin,
                    args,
                    span,
                });
            }

            Ok(Expr::Call {
                function: name.into(),
                args,
                kwargs,
                splat_mask,
                kwargs_splat_mask,
                span,
            })
        }
    }
}

/// Extract module and function names from a field expression for module-qualified calls.
/// Returns Some((module_name, func_name)) if the pattern is Module.func
///
/// Check if a name is a known module name.
///
/// This function uses multiple strategies to identify module names:
/// 1. Known built-in modules: Base, Core, Main, Pkg
/// 2. Known stdlib modules: Statistics, Test, Random, LinearAlgebra, etc.
/// 3. PascalCase heuristic: Names starting with uppercase letters are assumed to be modules
///    (Julia convention is PascalCase for modules, camelCase/snake_case for variables)
///
/// This allows us to distinguish Module.func(x) from obj.method(x) at lowering time
/// without requiring type information.
///
/// # Limitations
/// - User-defined modules with lowercase names may not be recognized
/// - Variables with PascalCase names may be incorrectly identified as modules
/// - This is a best-effort heuristic; for full correctness, type information
///   would need to be passed from the compile phase (see Issue #1360)
fn is_known_module_name(name: &str) -> bool {
    // Check built-in modules
    if matches!(name, "Base" | "Core" | "Main" | "Pkg" | "Sys") {
        return true;
    }

    // Check stdlib modules
    if stdlib::is_stdlib_module(name) {
        return true;
    }

    // Fall back to PascalCase heuristic for user-defined modules
    name.chars()
        .next()
        .map(|c| c.is_ascii_uppercase())
        .unwrap_or(false)
}

fn extract_module_call_target<'a>(
    walker: &CstWalker<'a>,
    node: Node<'a>,
) -> Option<(String, String)> {
    let named = walker.named_children_vec(&node);
    if named.len() < 2 {
        return None;
    }

    // Last child could be:
    // - An identifier (the function name): Module.func
    // - An operator for quoted operators: Base.:+, Base.:-
    let func_node = named[1];
    let func_text = walker.text(&func_node);
    let func_name = match walker.kind(&func_node) {
        NodeKind::Identifier => func_text.to_string(),
        NodeKind::Operator => {
            // Handle Base.:+ syntax - operator node contains just the operator symbol
            func_text.to_string()
        }
        NodeKind::QuoteExpression => {
            let quoted = func_text.strip_prefix(':').unwrap_or(func_text);
            if quoted.starts_with('(') && quoted.ends_with(')') && quoted.len() > 2 {
                quoted[1..quoted.len() - 1].to_string()
            } else {
                quoted.to_string()
            }
        }
        _ => {
            // For other node kinds, check if text starts with ':'
            // This handles quote expressions like :+, :-, etc.
            {
                let stripped = func_text.strip_prefix(':')?;
                stripped.to_string()
            }
        }
    };

    // First child could be:
    // - An identifier (simple case: Module.func)
    // - A FieldExpression (nested case: A.B.func)
    let module_node = named[0];
    let module_name = match walker.kind(&module_node) {
        NodeKind::Identifier => {
            let name = walker.text(&module_node).to_string();
            // Only treat as module call if it's a known module name
            // This allows c.f(x) where c is a struct with a function field
            if !is_known_module_name(&name) {
                return None;
            }
            name
        }
        NodeKind::FieldExpression => {
            // Nested module path: extract full path recursively
            let path = extract_nested_module_path(walker, module_node)?;
            // Check if the root of the path is a known module
            let root = path.split('.').next().unwrap_or(&path);
            if !is_known_module_name(root) {
                return None;
            }
            path
        }
        _ => return None,
    };

    Some((module_name, func_name))
}

/// Recursively extract nested module path from a FieldExpression.
/// For example, `A.B.C` returns "A.B.C".
fn extract_nested_module_path<'a>(walker: &CstWalker<'a>, node: Node<'a>) -> Option<String> {
    let named = walker.named_children_vec(&node);
    if named.len() < 2 {
        return None;
    }

    let left = named[0];
    let right = named[1];

    // Right side should be an identifier
    if walker.kind(&right) != NodeKind::Identifier {
        return None;
    }
    let right_name = walker.text(&right).to_string();

    // Left side could be an identifier or another FieldExpression
    let left_path = match walker.kind(&left) {
        NodeKind::Identifier => walker.text(&left).to_string(),
        NodeKind::FieldExpression => extract_nested_module_path(walker, left)?,
        _ => return None,
    };

    Some(format!("{}.{}", left_path, right_name))
}

/// Lower argument list without lambda context
pub fn lower_argument_list<'a>(walker: &CstWalker<'a>, node: Node<'a>) -> LowerResult<Vec<Expr>> {
    let mut args = Vec::new();
    for child in walker.named_children(&node) {
        // Skip DoClause (handled separately by caller)
        if walker.kind(&child) == NodeKind::DoClause {
            continue;
        }
        args.push(lower_expr(walker, child)?);
    }
    Ok(args)
}

/// Parse an argument list with both positional and keyword arguments.
/// Keyword arguments appear after a semicolon separator (`;`) or as `name=value` assignments.
/// Returns (positional_args, keyword_args, splat_mask, kwargs_splat_mask)
/// splat_mask[i] is true if positional_args[i] should be splatted at runtime.
/// kwargs_splat_mask[i] is true if kwargs[i] should be splatted at runtime.
pub fn lower_argument_list_with_kwargs<'a>(
    walker: &CstWalker<'a>,
    node: Node<'a>,
) -> LowerResult<(
    Vec<Expr>,
    Vec<(crate::ir::core::InternedStr, Expr)>,
    Vec<bool>,
    Vec<bool>,
)> {
    let mut positional_args = Vec::new();
    let mut kwargs = Vec::new();
    let mut splat_mask = Vec::new();
    let mut kwargs_splat_mask = Vec::new();
    let mut saw_semicolon = false;

    // Iterate through all children (including non-named like `;`)
    for child in walker.children(&node) {
        let kind_str = child.kind();

        // Check for semicolon separator
        if kind_str == ";" {
            saw_semicolon = true;
            continue;
        }

        // Skip non-named children (parentheses, commas)
        if !child.is_named() {
            continue;
        }

        let kind = walker.kind(&child);

        if saw_semicolon {
            // After semicolon: keyword arguments (assignments, KeywordArgument, or splat)
            match kind {
                NodeKind::Assignment => {
                    if let Some((name, value)) = parse_kwarg_assignment(walker, child, None)? {
                        kwargs.push((name, value));
                        kwargs_splat_mask.push(false);
                    }
                }
                NodeKind::KeywordArgument => {
                    // Pure Rust parser: KeywordArgument node with [Identifier, value] children
                    if let Some((name, value)) = parse_keyword_argument(walker, child, None)? {
                        kwargs.push((name, value));
                        kwargs_splat_mask.push(false);
                    }
                }
                NodeKind::SplatExpression => {
                    // Kwargs splat expression after semicolon: f(; opts...)
                    if let Some(inner) = walker.named_children(&child).next() {
                        // Use empty string as key to mark this as a splat
                        kwargs.push(("".to_string().into(), lower_expr(walker, inner)?));
                        kwargs_splat_mask.push(true);
                    }
                }
                _ => {
                    // Skip other nodes after semicolon (operators, etc.)
                }
            }
        } else {
            // Before semicolon: check if this is a keyword argument or positional
            match kind {
                NodeKind::Assignment => {
                    // Assignment before semicolon is also a kwarg (Julia allows `f(x, y=1)` without semicolon)
                    if let Some((name, value)) = parse_kwarg_assignment(walker, child, None)? {
                        kwargs.push((name, value));
                        kwargs_splat_mask.push(false);
                    }
                }
                NodeKind::KeywordArgument => {
                    // Pure Rust parser: KeywordArgument node
                    if let Some((name, value)) = parse_keyword_argument(walker, child, None)? {
                        kwargs.push((name, value));
                        kwargs_splat_mask.push(false);
                    }
                }
                NodeKind::SplatExpression => {
                    // Positional splat expression: args... - extract inner expression and mark for runtime expansion
                    if let Some(inner) = walker.named_children(&child).next() {
                        positional_args.push(lower_expr(walker, inner)?);
                        splat_mask.push(true);
                    }
                }
                NodeKind::ArrowFunctionExpression => {
                    positional_args.push(lower_arrow_value_as_nested(walker, child)?);
                    splat_mask.push(false);
                }
                NodeKind::Operator => {
                    // Bare operator as function argument: map(+, ...), reduce(*, ...)
                    // Convert to FunctionRef so it can be passed as a first-class function (Issue #1985)
                    let op_text = walker.text(&child).to_string();
                    let op_span = walker.span(&child);
                    positional_args.push(Expr::FunctionRef {
                        name: op_text.into(),
                        span: op_span,
                    });
                    splat_mask.push(false);
                }
                NodeKind::DoClause => {
                    // Skip DoClause (handled separately by caller)
                }
                _ => {
                    // Positional argument
                    positional_args.push(lower_expr(walker, child)?);
                    splat_mask.push(false);
                }
            }
        }
    }

    Ok((positional_args, kwargs, splat_mask, kwargs_splat_mask))
}

/// Parse an assignment node as a keyword argument (e.g., `y=1`).
/// Returns Some((name, value)) if valid, None if not a valid kwarg.
/// When lambda_ctx is provided, arrow functions in kwarg values are supported (Issue #2073).
fn parse_kwarg_assignment<'a>(
    walker: &CstWalker<'a>,
    node: Node<'a>,
    lambda_ctx: Option<&LambdaContext>,
) -> LowerResult<Option<(crate::ir::core::InternedStr, Expr)>> {
    // Filter out operator nodes to get [name, value]
    let children: Vec<_> = walker
        .named_children(&node)
        .filter(|n| walker.kind(n) != NodeKind::Operator)
        .collect();

    if children.len() < 2 {
        return Ok(None);
    }

    let name_node = children[0];
    let value_node = children[1];

    if walker.kind(&name_node) != NodeKind::Identifier {
        return Ok(None);
    }

    let name = walker.text(&name_node).to_string();
    let value = if let Some(ctx) = lambda_ctx {
        lower_expr_with_ctx(walker, value_node, ctx)?
    } else {
        lower_expr(walker, value_node)?
    };

    Ok(Some((name.into(), value)))
}

/// Parse a KeywordArgument node (Pure Rust parser format).
/// Structure: KeywordArgument { Identifier, value_expr }
/// When lambda_ctx is provided, arrow functions in kwarg values are supported (Issue #2073).
fn parse_keyword_argument<'a>(
    walker: &CstWalker<'a>,
    node: Node<'a>,
    lambda_ctx: Option<&LambdaContext>,
) -> LowerResult<Option<(crate::ir::core::InternedStr, Expr)>> {
    let children = walker.named_children_vec(&node);

    if children.len() < 2 {
        return Ok(None);
    }

    let name_node = children[0];
    let value_node = children[1];

    if walker.kind(&name_node) != NodeKind::Identifier {
        return Ok(None);
    }

    let name = walker.text(&name_node).to_string();
    let value = if let Some(ctx) = lambda_ctx {
        lower_expr_with_ctx(walker, value_node, ctx)?
    } else {
        lower_expr(walker, value_node)?
    };

    Ok(Some((name.into(), value)))
}
