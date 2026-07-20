//! Macro statement lowering
//!
//! Note: Most macros (@show, @assert, @something, @time, @evalpoly, etc.) are now
//! implemented in Pure Julia (base/macros.jl). This module handles:
//! - User-defined macro expansion
//! - Base macro expansion (from base/macros.jl)
//! - Stdlib macro expansion (from stdlib modules like Test)
//!
//! The following macros have been migrated to Pure Julia:
//! - @show, @assert, @something → base/macros.jl
//! - @time, @elapsed, @timed → base/macros.jl
//! - @evalpoly, @sprintf, @printf → base/macros.jl
//! - @test, @testset, @test_throws → stdlib/Test/src/Test.jl

mod enum_impl;
pub mod expand;
mod static_eval;

use crate::error::{UnsupportedFeature, UnsupportedFeatureKind};
use crate::ir::core::{Block, BuiltinOp, Expr, Function, Literal, MetaAnnotation, Stmt};
use crate::lowering::expr;
use crate::lowering::expr::quote::cst_to_macro_arg_constructor;
use crate::lowering::function;
use crate::lowering::LowerResult;
use crate::lowering::{internal_lowering_error, LambdaContext};
use crate::parser::cst::{CstWalker, Node, NodeKind};
use crate::types::JuliaType;
use std::collections::HashSet;
#[cfg(debug_assertions)]
use std::sync::OnceLock;

use enum_impl::lower_enum_macro_with_ctx;
use expand::{
    expand_base_macro, expand_bundled_package_macro, expand_stdlib_macro, expand_user_defined_macro,
};
use static_eval::lower_static_macro_with_ctx;

#[cfg(debug_assertions)]
const MACRO_DEBUG_ENV: &str = "SJULIA_MACRO_DEBUG";

#[cfg(debug_assertions)]
fn macro_debug_enabled() -> bool {
    static ENABLED: OnceLock<bool> = OnceLock::new();
    *ENABLED.get_or_init(|| std::env::var(MACRO_DEBUG_ENV).is_ok())
}

#[cfg(debug_assertions)]
pub(super) fn macro_debug_log(args: std::fmt::Arguments<'_>) {
    if macro_debug_enabled() {
        use std::io::Write;
        let _ = writeln!(std::io::stderr(), "{args}");
    }
}

fn macro_args<'a>(
    walker: &CstWalker<'a>,
    node: Node<'a>,
    args_node: Option<Node<'a>>,
    direct_args: &[Node<'a>],
) -> Vec<Node<'a>> {
    if let Some(args_node) = args_node {
        walker.named_children_vec(&args_node)
    } else if !direct_args.is_empty() {
        direct_args.to_vec()
    } else {
        walker
            .named_children(&node)
            .filter(|child| walker.kind(child) != NodeKind::MacroIdentifier)
            .collect()
    }
}

fn assignment_lhs_rhs<'a>(walker: &CstWalker<'a>, node: Node<'a>) -> Option<(Node<'a>, Node<'a>)> {
    let all_children = walker.children(&node);
    for (i, child) in all_children.iter().enumerate() {
        if child.kind() == "operator"
            && walker.text(child) == "="
            && i > 0
            && i + 1 < all_children.len()
        {
            return Some((all_children[i - 1], all_children[i + 1]));
        }
    }

    let named = walker.named_children_vec(&node);
    if named.len() >= 2 {
        Some((named[0], named[named.len() - 1]))
    } else {
        None
    }
}

fn lower_inbounds_index_assignment<'a>(
    walker: &CstWalker<'a>,
    node: Node<'a>,
    span: crate::span::Span,
    lambda_ctx: &LambdaContext,
) -> LowerResult<Option<Stmt>> {
    let Some((lhs, rhs)) = assignment_lhs_rhs(walker, node) else {
        return Ok(None);
    };
    if walker.kind(&lhs) != NodeKind::IndexExpression {
        return Ok(None);
    }

    let (array_name, index_nodes) = expr::extract_index_target(walker, lhs).ok_or_else(|| {
        UnsupportedFeature::new(
            UnsupportedFeatureKind::UnsupportedAssignmentTarget,
            walker.span(&lhs),
        )
    })?;

    let mut call_args = vec![
        Expr::Var(array_name.into(), walker.span(&lhs)),
        expr::lower_expr_with_ctx(walker, rhs, lambda_ctx)?,
    ];
    for index_node in index_nodes {
        call_args.push(expr::lower_expr_with_ctx(walker, index_node, lambda_ctx)?);
    }

    Ok(Some(Stmt::Expr {
        expr: Expr::Call {
            function: "#__sjulia_inbounds__".to_string().into(),
            args: vec![Expr::Call {
                function: "setindex!".to_string().into(),
                args: call_args,
                kwargs: vec![],
                splat_mask: vec![],
                kwargs_splat_mask: vec![],
                span,
            }],
            kwargs: vec![],
            splat_mask: vec![],
            kwargs_splat_mask: vec![],
            span,
        },
        span,
    }))
}

fn lower_inbounds_tuple_index_assignment<'a>(
    walker: &CstWalker<'a>,
    node: Node<'a>,
    span: crate::span::Span,
    lambda_ctx: &LambdaContext,
) -> LowerResult<Option<Stmt>> {
    let elements = walker.named_children_vec(&node);
    if elements.len() != 3 || walker.kind(&elements[1]) != NodeKind::Assignment {
        return Ok(None);
    }

    let Some((second_lhs, first_rhs)) = assignment_lhs_rhs(walker, elements[1]) else {
        return Ok(None);
    };
    if walker.kind(&elements[0]) == NodeKind::Identifier
        && walker.kind(&second_lhs) == NodeKind::Identifier
    {
        let first_var = walker.text(&elements[0]).to_string();
        let second_var = walker.text(&second_lhs).to_string();
        let first_temp = format!("__inbounds_tuple_tmp_{}_0", span.start);
        let second_temp = format!("__inbounds_tuple_tmp_{}_1", span.start);
        let first_rhs = expr::lower_expr_with_ctx(walker, first_rhs, lambda_ctx)?;
        let second_rhs = expr::lower_expr_with_ctx(walker, elements[2], lambda_ctx)?;
        return Ok(Some(Stmt::Block(Block {
            stmts: vec![
                Stmt::Assign {
                    var: first_temp.clone(),
                    value: inbounds_expr(first_rhs, span),
                    span,
                },
                Stmt::Assign {
                    var: second_temp.clone(),
                    value: inbounds_expr(second_rhs, span),
                    span,
                },
                Stmt::Assign {
                    var: first_var,
                    value: Expr::Var(first_temp.into(), span),
                    span,
                },
                Stmt::Assign {
                    var: second_var,
                    value: Expr::Var(second_temp.into(), span),
                    span,
                },
            ],
            span,
        })));
    }
    if walker.kind(&elements[0]) != NodeKind::IndexExpression {
        return Ok(None);
    }
    if walker.kind(&second_lhs) != NodeKind::IndexExpression {
        return Ok(None);
    }

    let lower_target = |target: Node<'a>| -> LowerResult<(String, Vec<Expr>)> {
        let (array_name, index_nodes) =
            expr::extract_index_target(walker, target).ok_or_else(|| {
                UnsupportedFeature::new(
                    UnsupportedFeatureKind::UnsupportedAssignmentTarget,
                    walker.span(&target),
                )
            })?;
        let indices = index_nodes
            .into_iter()
            .map(|index| expr::lower_expr_with_ctx(walker, index, lambda_ctx))
            .collect::<LowerResult<Vec<_>>>()?;
        Ok((array_name, indices))
    };

    let (first_array, first_indices) = lower_target(elements[0])?;
    let (second_array, second_indices) = lower_target(second_lhs)?;
    let first_temp = format!("__inbounds_tuple_tmp_{}_0", span.start);
    let second_temp = format!("__inbounds_tuple_tmp_{}_1", span.start);
    let first_rhs = expr::lower_expr_with_ctx(walker, first_rhs, lambda_ctx)?;
    let second_rhs = expr::lower_expr_with_ctx(walker, elements[2], lambda_ctx)?;

    Ok(Some(Stmt::Block(Block {
        stmts: vec![
            Stmt::Assign {
                var: first_temp.clone(),
                value: inbounds_expr(first_rhs, span),
                span,
            },
            Stmt::Assign {
                var: second_temp.clone(),
                value: inbounds_expr(second_rhs, span),
                span,
            },
            inbounds_setindex_stmt(
                first_array,
                first_indices,
                Expr::Var(first_temp.into(), span),
                span,
            ),
            inbounds_setindex_stmt(
                second_array,
                second_indices,
                Expr::Var(second_temp.into(), span),
                span,
            ),
        ],
        span,
    })))
}

fn declare_const_stmt(name: &str, span: crate::span::Span) -> Stmt {
    Stmt::Expr {
        expr: Expr::Call {
            function: "#__sjulia_declare_const__".to_string().into(),
            args: vec![Expr::Literal(Literal::Str(name.to_string()), span)],
            kwargs: Vec::new(),
            splat_mask: vec![false],
            kwargs_splat_mask: Vec::new(),
            span,
        },
        span,
    }
}

fn irrational_conversion_method(
    function_name: &str,
    type_name: &str,
    value: Expr,
    span: crate::span::Span,
) -> Stmt {
    Stmt::FunctionDef {
        func: Box::new(crate::ir::core::Function {
            name: function_name.to_string(),
            params: vec![crate::ir::core::TypedParam::new(
                "__irrational".to_string(),
                Some(JuliaType::Struct(type_name.to_string())),
                span,
            )],
            kwparams: Vec::new(),
            type_params: Vec::new(),
            return_type: None,
            body: Block {
                stmts: vec![Stmt::Return {
                    value: Some(Expr::Call {
                        function: function_name.to_string().into(),
                        args: vec![value],
                        kwargs: Vec::new(),
                        splat_mask: vec![false],
                        kwargs_splat_mask: Vec::new(),
                        span,
                    }),
                    span,
                }],
                span,
            },
            is_base_extension: false,
            is_runtime_eval: false,
            span,
            new_struct_name: None,
        }),
        span,
    }
}

fn lower_irrational_macro<'a>(
    walker: &CstWalker<'a>,
    node: Node<'a>,
    args_node: Option<Node<'a>>,
    direct_args: &[Node<'a>],
    span: crate::span::Span,
    lambda_ctx: &LambdaContext,
) -> LowerResult<Stmt> {
    let args = macro_args(walker, node, args_node, direct_args);
    if args.len() < 2 {
        return Err(
            UnsupportedFeature::new(UnsupportedFeatureKind::MacroCall, span)
                .with_hint("@irrational requires a symbol name and definition"),
        );
    }

    let symbol = args[0];
    if walker.kind(&symbol) != NodeKind::Identifier {
        return Err(UnsupportedFeature::new(
            UnsupportedFeatureKind::MacroCall,
            walker.span(&symbol),
        )
        .with_hint("@irrational first argument must be an identifier"));
    }

    let name = walker.text(&symbol).to_string();
    let type_name = format!("Irrational{{:{name}}}");
    let (float_value_node, big_value_node) = if args.len() >= 3 {
        (args[1], args[2])
    } else {
        (args[1], args[1])
    };
    let float_value = expr::lower_expr_with_ctx(walker, float_value_node, lambda_ctx)?;
    let big_value = expr::lower_expr_with_ctx(walker, big_value_node, lambda_ctx)?;

    Ok(Stmt::Block(Block {
        stmts: vec![
            declare_const_stmt(&name, span),
            Stmt::Assign {
                var: name.clone(),
                value: Expr::Call {
                    function: type_name.clone().into(),
                    args: Vec::new(),
                    kwargs: Vec::new(),
                    splat_mask: Vec::new(),
                    kwargs_splat_mask: Vec::new(),
                    span,
                },
                span,
            },
            irrational_conversion_method("Float64", &type_name, float_value.clone(), span),
            irrational_conversion_method("Float32", &type_name, float_value, span),
            irrational_conversion_method("BigFloat", &type_name, big_value, span),
        ],
        span,
    }))
}

fn inbounds_expr(expr: Expr, span: crate::span::Span) -> Expr {
    Expr::Call {
        function: "#__sjulia_inbounds__".to_string().into(),
        args: vec![expr],
        kwargs: vec![],
        splat_mask: vec![],
        kwargs_splat_mask: vec![],
        span,
    }
}

fn inbounds_setindex_stmt(
    array: String,
    indices: Vec<Expr>,
    value: Expr,
    span: crate::span::Span,
) -> Stmt {
    let mut args = Vec::with_capacity(indices.len() + 2);
    args.push(Expr::Var(array.into(), span));
    args.push(value);
    args.extend(indices);

    Stmt::Expr {
        expr: inbounds_expr(
            Expr::Call {
                function: "setindex!".to_string().into(),
                args,
                kwargs: vec![],
                splat_mask: vec![],
                kwargs_splat_mask: vec![],
                span,
            },
            span,
        ),
        span,
    }
}

fn mark_block_inbounds(block: crate::ir::core::Block) -> crate::ir::core::Block {
    crate::ir::core::Block {
        stmts: block.stmts.into_iter().map(mark_stmt_inbounds).collect(),
        span: block.span,
    }
}

fn mark_stmt_inbounds(stmt: Stmt) -> Stmt {
    match stmt {
        Stmt::Block(block) => Stmt::Block(mark_block_inbounds(block)),
        Stmt::Assign { var, value, span } => Stmt::Assign {
            var,
            value: inbounds_expr(value, span),
            span,
        },
        Stmt::AddAssign { var, value, span } => Stmt::AddAssign {
            var,
            value: inbounds_expr(value, span),
            span,
        },
        Stmt::Expr { expr, span } => Stmt::Expr {
            expr: inbounds_expr(expr, span),
            span,
        },
        Stmt::IndexAssign {
            array,
            indices,
            value,
            span,
        } => inbounds_setindex_stmt(array, indices, value, span),
        Stmt::Return {
            value: Some(value),
            span,
        } => Stmt::Return {
            value: Some(inbounds_expr(value, span)),
            span,
        },
        Stmt::For {
            var,
            start,
            end,
            step,
            body,
            span,
        } => Stmt::For {
            var,
            start: inbounds_expr(start, span),
            end: inbounds_expr(end, span),
            step: step.map(|expr| inbounds_expr(expr, span)),
            body: mark_block_inbounds(body),
            span,
        },
        Stmt::ForEach {
            var,
            iterable,
            body,
            span,
        } => Stmt::ForEach {
            var,
            iterable: inbounds_expr(iterable, span),
            body: mark_block_inbounds(body),
            span,
        },
        Stmt::ForEachTuple {
            vars,
            iterable,
            body,
            span,
        } => Stmt::ForEachTuple {
            vars,
            iterable: inbounds_expr(iterable, span),
            body: mark_block_inbounds(body),
            span,
        },
        Stmt::While {
            condition,
            body,
            span,
        } => Stmt::While {
            condition: inbounds_expr(condition, span),
            body: mark_block_inbounds(body),
            span,
        },
        Stmt::If {
            condition,
            then_branch,
            else_branch,
            span,
        } => Stmt::If {
            condition: inbounds_expr(condition, span),
            then_branch: mark_block_inbounds(then_branch),
            else_branch: else_branch.map(mark_block_inbounds),
            span,
        },
        Stmt::Try {
            try_block,
            catch_var,
            catch_block,
            else_block,
            finally_block,
            span,
        } => Stmt::Try {
            try_block: mark_block_inbounds(try_block),
            catch_var,
            catch_block: catch_block.map(mark_block_inbounds),
            else_block: else_block.map(mark_block_inbounds),
            finally_block: finally_block.map(mark_block_inbounds),
            span,
        },
        Stmt::Timed { body, span } => Stmt::Timed {
            body: mark_block_inbounds(body),
            span,
        },
        Stmt::Test {
            condition,
            message,
            span,
        } => Stmt::Test {
            condition: inbounds_expr(condition, span),
            message,
            span,
        },
        Stmt::TestSet { name, body, span } => Stmt::TestSet {
            name,
            body: mark_block_inbounds(body),
            span,
        },
        Stmt::TestThrows {
            exception_type,
            expr,
            span,
        } => Stmt::TestThrows {
            exception_type,
            expr: Box::new(inbounds_expr(*expr, span)),
            span,
        },
        Stmt::FieldAssign {
            object,
            field,
            value,
            span,
        } => Stmt::FieldAssign {
            object,
            field,
            value: inbounds_expr(value, span),
            span,
        },
        Stmt::DestructuringAssign {
            targets,
            value,
            span,
        } => Stmt::DestructuringAssign {
            targets,
            value: inbounds_expr(value, span),
            span,
        },
        Stmt::DictAssign {
            dict,
            key,
            value,
            span,
        } => Stmt::DictAssign {
            dict,
            key: inbounds_expr(key, span),
            value: inbounds_expr(value, span),
            span,
        },
        other => other,
    }
}

/// Lower `@code_warntype f(args...)` into `code_warntype(f, typeof((args...,)))`
/// (Issue #5145).
///
/// Upstream Julia's `@code_warntype` extracts the call's argument types and
/// forwards them to `code_warntype(f, Tuple{...})`. SubsetJuliaVM does not run
/// programmatic AST destructuring inside pure-Julia macros, so the call is
/// reconstructed here: the callee becomes the function value, and the evaluated
/// argument expressions are wrapped in a tuple whose runtime `typeof` is the
/// signature tuple type accepted by `code_warntype` / `infer_return_type`.
fn lower_code_warntype_macro<'a>(
    walker: &CstWalker<'a>,
    args: &[Node<'a>],
    span: crate::span::Span,
    lambda_ctx: &LambdaContext,
) -> LowerResult<Expr> {
    if args.len() != 1 {
        return Err(
            UnsupportedFeature::new(UnsupportedFeatureKind::MacroCall, span).with_hint(
                "@code_warntype expects a single function call: @code_warntype f(args...)",
            ),
        );
    }

    let call_node = args[0];
    if walker.kind(&call_node) != NodeKind::CallExpression {
        return Err(
            UnsupportedFeature::new(UnsupportedFeatureKind::MacroCall, span)
                .with_hint("@code_warntype expects a function call: @code_warntype f(args...)"),
        );
    }

    // Named children of a call: [callee, ArgumentList?]. Lower the callee to a
    // function value and the argument list into evaluated expressions.
    let named = walker.named_children_vec(&call_node);
    if named.is_empty() {
        return Err(
            UnsupportedFeature::new(UnsupportedFeatureKind::MacroCall, span)
                .with_hint("@code_warntype: could not find the called function"),
        );
    }
    let callee = named[0];
    let func_expr = expr::lower_expr_with_ctx(walker, callee, lambda_ctx)?;

    let arg_nodes: Vec<Node<'a>> = named
        .iter()
        .skip(1)
        .filter(|n| {
            matches!(
                walker.kind(n),
                NodeKind::ArgumentList | NodeKind::TupleExpression
            )
        })
        .flat_map(|n| walker.named_children_vec(n))
        .collect();

    let arg_exprs: Vec<Expr> = arg_nodes
        .iter()
        .map(|n| expr::lower_expr_with_ctx(walker, *n, lambda_ctx))
        .collect::<Result<Vec<_>, _>>()?;

    // typeof((args...,)) yields the signature tuple type, e.g. Tuple{Int64, Float64}.
    let tuple_expr = Expr::TupleLiteral {
        elements: arg_exprs,
        span,
    };
    let types_expr = Expr::Call {
        function: "typeof".to_string().into(),
        args: vec![tuple_expr],
        kwargs: vec![],
        splat_mask: vec![],
        kwargs_splat_mask: vec![],
        span,
    };

    Ok(Expr::Call {
        function: "code_warntype".to_string().into(),
        args: vec![func_expr, types_expr],
        kwargs: vec![],
        splat_mask: vec![],
        kwargs_splat_mask: vec![],
        span,
    })
}

fn unsupported_opaque_closure_macro(span: crate::span::Span) -> UnsupportedFeature {
    UnsupportedFeature::new(UnsupportedFeatureKind::MacroCall, span).with_hint(
        "opaque closures via Base.Experimental.@opaque are not supported yet (Issue #4289)",
    )
}

fn macro_symbol_constructor(name: &str, span: crate::span::Span) -> Expr {
    Expr::Builtin {
        name: BuiltinOp::SymbolNew,
        args: vec![Expr::Literal(Literal::Str(name.to_string()), span)],
        span,
    }
}

fn macro_globalref_constructor(module: &str, name: &str, span: crate::span::Span) -> Expr {
    Expr::Builtin {
        name: BuiltinOp::GlobalRefNew,
        args: vec![
            macro_symbol_constructor(module, span),
            macro_symbol_constructor(name, span),
        ],
        span,
    }
}

fn macro_expr_constructor(head: &str, mut args: Vec<Expr>, span: crate::span::Span) -> Expr {
    args.insert(0, macro_symbol_constructor(head, span));
    Expr::Builtin {
        name: BuiltinOp::ExprNew,
        args,
        span,
    }
}

fn lower_eval_module_macro<'a>(
    walker: &CstWalker<'a>,
    args: &[Node<'a>],
    span: crate::span::Span,
) -> LowerResult<Option<Stmt>> {
    if args.len() != 2 {
        return Ok(None);
    }

    let module_arg = cst_to_macro_arg_constructor(walker, args[0])?;
    let body = cst_to_macro_arg_constructor(walker, args[1])?;
    let quoted_body = macro_expr_constructor("quote", vec![body], span);
    let core_eval = macro_globalref_constructor("Core", "eval", span);
    let call = macro_expr_constructor("call", vec![core_eval, module_arg, quoted_body], span);

    Ok(Some(Stmt::Expr {
        expr: Expr::Builtin {
            name: BuiltinOp::Eval,
            args: vec![call],
            span,
        },
        span,
    }))
}

fn lower_eval_function_definition<'a>(
    walker: &CstWalker<'a>,
    args: &[Node<'a>],
    span: crate::span::Span,
    lambda_ctx: &LambdaContext,
) -> LowerResult<Option<Stmt>> {
    if args.len() != 1 {
        return Ok(None);
    }

    let def_node = args[0];
    let funcs = lambda_ctx.with_new_struct_authority(None, || {
        if walker.kind(&def_node) == NodeKind::FunctionDefinition {
            crate::lowering::lower_function_all_with_ctx_if_needed(walker, def_node, lambda_ctx)
        } else if function::is_short_function_definition(walker, def_node) {
            crate::lowering::lower_short_function_all_with_ctx_if_needed(
                walker, def_node, lambda_ctx,
            )
        } else {
            Ok(Vec::new())
        }
    })?;
    if funcs.is_empty() {
        return Ok(None);
    }

    let mut stmts = funcs
        .into_iter()
        .map(|mut func| {
            func.is_runtime_eval = true;
            let span = func.span;
            Stmt::EvalFunctionDef {
                func: Box::new(func),
                span,
            }
        })
        .collect::<Vec<_>>();

    Ok(Some(if stmts.len() == 1 {
        stmts.pop().ok_or_else(|| {
            internal_lowering_error(span, "eval function stmt count checked above")
        })?
    } else {
        Stmt::Block(Block { stmts, span })
    }))
}

fn lower_meta_annotation_compat<'a>(
    walker: &CstWalker<'a>,
    macro_name: &str,
    node: Node<'a>,
    args_node: Option<Node<'a>>,
    direct_args: &[Node<'a>],
    span: crate::span::Span,
    lambda_ctx: &LambdaContext,
) -> LowerResult<Stmt> {
    let args = macro_args(walker, node, args_node, direct_args);
    if args.is_empty() && matches!(macro_name, "inline" | "noinline") {
        return Ok(lower_meta_marker_stmt(macro_name, Vec::new(), span));
    }
    if args.len() != 1 {
        return Err(
            UnsupportedFeature::new(UnsupportedFeatureKind::MacroCall, span).with_hint(format!(
                "@{} no-op compatibility currently requires exactly one argument",
                macro_name
            )),
        );
    }

    let arg = args[0];
    if macro_name == "boundscheck" {
        let stmt = super::lower_stmt_with_ctx(walker, arg, lambda_ctx)?;
        return Ok(Stmt::If {
            condition: Expr::Call {
                function: "#__sjulia_boundscheck_enabled__".to_string().into(),
                args: vec![],
                kwargs: vec![],
                splat_mask: vec![],
                kwargs_splat_mask: vec![],
                span,
            },
            then_branch: crate::ir::core::Block {
                stmts: vec![stmt],
                span,
            },
            else_branch: None,
            span,
        });
    }

    lower_meta_target_compat(walker, macro_name, Vec::new(), arg, span, lambda_ctx)
}

fn lower_meta_marker_stmt(name: &str, args: Vec<String>, span: crate::span::Span) -> Stmt {
    Stmt::Meta {
        annotation: MetaAnnotation {
            name: name.to_string(),
            args,
        },
        span,
    }
}

fn is_short_function_def<'a>(walker: &CstWalker<'a>, node: Node<'a>) -> bool {
    walker.kind(&node) == NodeKind::Assignment
        && function::is_short_function_definition(walker, node)
}

fn lower_meta_target_compat<'a>(
    walker: &CstWalker<'a>,
    macro_name: &str,
    marker_args: Vec<String>,
    arg: Node<'a>,
    span: crate::span::Span,
    lambda_ctx: &LambdaContext,
) -> LowerResult<Stmt> {
    let marker = lower_meta_marker_stmt(macro_name, marker_args, span);
    let target = match walker.kind(&arg) {
        NodeKind::FunctionDefinition
        | NodeKind::ShortFunctionDefinition
        | NodeKind::MacroCall
        | NodeKind::ForStatement
        | NodeKind::WhileStatement
        | NodeKind::IfStatement
        | NodeKind::Assignment => super::lower_stmt_with_ctx(walker, arg, lambda_ctx)?,
        _ => {
            let expr = expr::lower_expr_with_ctx(walker, arg, lambda_ctx)?;
            Stmt::Expr { expr, span }
        }
    };
    if let Some(stmt) = attach_meta_to_function_defs(target.clone(), marker.clone()) {
        return Ok(stmt);
    }
    Ok(Stmt::Block(crate::ir::core::Block {
        stmts: vec![marker, target],
        span,
    }))
}

fn attach_meta_to_function_defs(stmt: Stmt, marker: Stmt) -> Option<Stmt> {
    match stmt {
        Stmt::FunctionDef { mut func, span } => {
            func.body.stmts.insert(0, marker);
            Some(Stmt::FunctionDef { func, span })
        }
        Stmt::Block(mut block)
            if block
                .stmts
                .iter()
                .all(|stmt| matches!(stmt, Stmt::FunctionDef { .. })) =>
        {
            for stmt in &mut block.stmts {
                if let Stmt::FunctionDef { func, .. } = stmt {
                    func.body.stmts.insert(0, marker.clone());
                }
            }
            Some(Stmt::Block(block))
        }
        _ => None,
    }
}

fn lower_constprop_compat<'a>(
    walker: &CstWalker<'a>,
    node: Node<'a>,
    args_node: Option<Node<'a>>,
    direct_args: &[Node<'a>],
    span: crate::span::Span,
    lambda_ctx: &LambdaContext,
) -> LowerResult<Stmt> {
    let args = macro_args(walker, node, args_node, direct_args);
    match args.as_slice() {
        [setting] => Ok(lower_meta_marker_stmt(
            "constprop",
            vec![walker.text(setting).to_string()],
            span,
        )),
        [setting, target]
            if matches!(
                walker.kind(target),
                NodeKind::FunctionDefinition | NodeKind::ShortFunctionDefinition | NodeKind::MacroCall
            ) || is_short_function_def(walker, *target) =>
        {
            lower_meta_target_compat(
                walker,
                "constprop",
                vec![walker.text(setting).to_string()],
                *target,
                span,
                lambda_ctx,
            )
        }
        [_setting, _target] => Err(
            UnsupportedFeature::new(UnsupportedFeatureKind::MacroCall, span).with_hint(
                "@constprop compatibility currently supports function definitions or metadata-only settings",
            ),
        ),
        _ => Err(
            UnsupportedFeature::new(UnsupportedFeatureKind::MacroCall, span).with_hint(
                "@constprop compatibility expects `@constprop setting [function-definition]`",
            ),
        ),
    }
}

fn lower_assume_effects_compat<'a>(
    walker: &CstWalker<'a>,
    node: Node<'a>,
    args_node: Option<Node<'a>>,
    direct_args: &[Node<'a>],
    span: crate::span::Span,
    lambda_ctx: &LambdaContext,
) -> LowerResult<Stmt> {
    let args = macro_args(walker, node, args_node, direct_args);
    let Some(target) = args.last().copied() else {
        return Err(
            UnsupportedFeature::new(UnsupportedFeatureKind::MacroCall, span)
                .with_hint("@assume_effects requires at least one metadata setting"),
        );
    };

    if walker.kind(&target) == NodeKind::QuoteExpression {
        let marker_args = args
            .iter()
            .map(|arg| walker.text(arg).to_string())
            .collect();
        return Ok(lower_meta_marker_stmt("assume_effects", marker_args, span));
    }

    let marker_args = args[..args.len().saturating_sub(1)]
        .iter()
        .map(|arg| walker.text(arg).to_string())
        .collect();
    lower_meta_target_compat(
        walker,
        "assume_effects",
        marker_args,
        target,
        span,
        lambda_ctx,
    )
}

fn lower_generated_function_compat<'a>(
    walker: &CstWalker<'a>,
    node: Node<'a>,
    args_node: Option<Node<'a>>,
    direct_args: &[Node<'a>],
    span: crate::span::Span,
    lambda_ctx: &LambdaContext,
) -> LowerResult<Stmt> {
    let args = macro_args(walker, node, args_node, direct_args);
    if args.len() != 1 {
        return Err(
            UnsupportedFeature::new(UnsupportedFeatureKind::MacroCall, span).with_hint(
                "@generated compatibility currently supports one function definition only",
            ),
        );
    }

    let def_node = args[0];
    let (mut funcs, mark_generated) = if walker.kind(&def_node) == NodeKind::FunctionDefinition {
        // Route through the central LambdaContext authority so a `where`
        // binder (including a builtin-spelled one) or macro call inside the
        // generated body keeps its lexical context (Issue #10936).
        let mut funcs =
            crate::lowering::lower_function_all_with_ctx_if_needed(walker, def_node, lambda_ctx)?;
        let mut unquoted = false;
        let mut unquoted_requires_generated_frame = false;
        if let Some(block_node) = walker.find_child(&def_node, NodeKind::Block) {
            unquoted_requires_generated_frame =
                super::control_if::unquoted_generated_block_requires_generated_frame(
                    walker, block_node,
                );
            if let Some(unquoted_body) =
                super::control_if::try_unquote_generated_block(walker, block_node)
            {
                if let Some(func) = funcs.first_mut() {
                    func.body = unquoted_body?;
                    unquoted = true;
                }
            }
        }
        if !unquoted {
            for func in funcs.iter_mut() {
                wrap_generated_returns_with_eval(&mut func.body);
            }
        }
        (funcs, !unquoted || unquoted_requires_generated_frame)
    } else if function::is_short_function_definition(walker, def_node) {
        // See the FunctionDefinition arm above (Issue #10936).
        let mut funcs = crate::lowering::lower_short_function_all_with_ctx_if_needed(
            walker, def_node, lambda_ctx,
        )?;
        let mut unquoted = false;
        let mut unquoted_requires_generated_frame = false;
        let children = walker.named_children_vec(&def_node);
        if let Some(rhs_node) = children.last() {
            unquoted_requires_generated_frame =
                super::control_if::unquoted_generated_short_body_requires_generated_frame(
                    walker, *rhs_node,
                );
            if let Some(unquoted_body) =
                super::control_if::try_unquote_generated_short_body(walker, *rhs_node)
            {
                if let Some(func) = funcs.first_mut() {
                    func.body = unquoted_body?;
                    unquoted = true;
                }
            }
        }
        if !unquoted {
            for func in funcs.iter_mut() {
                wrap_generated_returns_with_eval(&mut func.body);
            }
        }
        (funcs, !unquoted || unquoted_requires_generated_frame)
    } else {
        return Err(
            UnsupportedFeature::new(UnsupportedFeatureKind::MacroCall, span)
                .with_hint("@generated compatibility currently supports function definitions only"),
        );
    };
    if mark_generated {
        for func in &mut funcs {
            mark_generated_function(func);
        }
    }

    Ok(super::lower_function_defs_to_stmt(funcs, span))
}

fn mark_generated_function(func: &mut Function) {
    func.body.stmts.insert(
        0,
        Stmt::Meta {
            annotation: MetaAnnotation {
                name: "generated".to_string(),
                args: vec![],
            },
            span: func.span,
        },
    );
}

fn wrap_generated_returns_with_eval(block: &mut Block) {
    let mut expr_locals = HashSet::new();
    collect_generated_expr_locals(block, &mut expr_locals);
    wrap_generated_returns_with_eval_impl(block, &expr_locals);
    wrap_generated_tail_expr_with_eval(block, &expr_locals);
}

fn collect_generated_expr_locals(block: &Block, expr_locals: &mut HashSet<String>) {
    for stmt in &block.stmts {
        match stmt {
            Stmt::Assign { var, value, .. } if is_generated_expr_value(value, expr_locals) => {
                expr_locals.insert(var.clone());
            }
            Stmt::Block(nested)
            | Stmt::For { body: nested, .. }
            | Stmt::ForEach { body: nested, .. }
            | Stmt::ForEachTuple { body: nested, .. }
            | Stmt::While { body: nested, .. } => {
                collect_generated_expr_locals(nested, expr_locals)
            }
            Stmt::If {
                then_branch,
                else_branch,
                ..
            } => {
                collect_generated_expr_locals(then_branch, expr_locals);
                if let Some(else_branch) = else_branch {
                    collect_generated_expr_locals(else_branch, expr_locals);
                }
            }
            Stmt::Try {
                try_block,
                catch_block,
                else_block,
                finally_block,
                ..
            } => {
                collect_generated_expr_locals(try_block, expr_locals);
                if let Some(catch_block) = catch_block {
                    collect_generated_expr_locals(catch_block, expr_locals);
                }
                if let Some(else_block) = else_block {
                    collect_generated_expr_locals(else_block, expr_locals);
                }
                if let Some(finally_block) = finally_block {
                    collect_generated_expr_locals(finally_block, expr_locals);
                }
            }
            _ => {}
        }
    }
}

fn wrap_generated_returns_with_eval_impl(block: &mut Block, expr_locals: &HashSet<String>) {
    for stmt in &mut block.stmts {
        match stmt {
            Stmt::Return {
                value: Some(value),
                span,
            } if !is_eval_call(value) && is_generated_expr_value(value, expr_locals) => {
                let original = value.clone();
                *value = eval_wrapped_expr(original, *span);
            }
            Stmt::Block(nested)
            | Stmt::For { body: nested, .. }
            | Stmt::ForEach { body: nested, .. }
            | Stmt::ForEachTuple { body: nested, .. }
            | Stmt::While { body: nested, .. } => {
                wrap_generated_returns_with_eval_impl(nested, expr_locals)
            }
            Stmt::If {
                then_branch,
                else_branch,
                ..
            } => {
                wrap_generated_returns_with_eval_impl(then_branch, expr_locals);
                if let Some(else_branch) = else_branch {
                    wrap_generated_returns_with_eval_impl(else_branch, expr_locals);
                }
            }
            Stmt::Try {
                try_block,
                catch_block,
                else_block,
                finally_block,
                ..
            } => {
                wrap_generated_returns_with_eval_impl(try_block, expr_locals);
                if let Some(catch_block) = catch_block {
                    wrap_generated_returns_with_eval_impl(catch_block, expr_locals);
                }
                if let Some(else_block) = else_block {
                    wrap_generated_returns_with_eval_impl(else_block, expr_locals);
                }
                if let Some(finally_block) = finally_block {
                    wrap_generated_returns_with_eval_impl(finally_block, expr_locals);
                }
            }
            _ => {}
        }
    }
}

fn wrap_generated_tail_expr_with_eval(block: &mut Block, expr_locals: &HashSet<String>) {
    if let Some(Stmt::Expr { expr, span }) = block.stmts.last_mut() {
        if !is_eval_call(expr) && is_generated_expr_value(expr, expr_locals) {
            let original = expr.clone();
            *expr = eval_wrapped_expr(original, *span);
        }
    }
}

fn eval_wrapped_expr(expr: Expr, span: crate::span::Span) -> Expr {
    Expr::Builtin {
        name: BuiltinOp::GeneratedEval,
        args: vec![expr],
        span,
    }
}

fn is_generated_expr_value(expr: &Expr, expr_locals: &HashSet<String>) -> bool {
    match expr {
        Expr::QuoteLiteral { .. } => true,
        Expr::Builtin {
            name: BuiltinOp::ExprNew,
            ..
        } => true,
        Expr::Ternary {
            then_expr,
            else_expr,
            ..
        } => {
            is_generated_expr_value(then_expr, expr_locals)
                && is_generated_expr_value(else_expr, expr_locals)
        }
        Expr::Var(name, _) => expr_locals.contains(name.as_str()),
        _ => false,
    }
}

fn is_eval_call(expr: &Expr) -> bool {
    match expr {
        Expr::Call {
            function,
            args,
            kwargs,
            splat_mask,
            kwargs_splat_mask,
            ..
        } => {
            function == "eval"
                && args.len() == 1
                && kwargs.is_empty()
                && splat_mask == &[false]
                && kwargs_splat_mask.is_empty()
        }
        Expr::Builtin {
            name: BuiltinOp::Eval | BuiltinOp::GeneratedEval,
            args,
            ..
        } => args.len() == 1,
        _ => false,
    }
}

pub fn lower_macro<'a>(walker: &CstWalker<'a>, node: Node<'a>) -> LowerResult<Stmt> {
    // Create an empty lambda context to support Base macros
    // This is used when lowering Base library itself where macros like @assert are used
    let lambda_ctx = LambdaContext::new();
    lower_macro_with_ctx(walker, node, &lambda_ctx)
}

// ==================== Lambda Context Versions ====================

pub fn lower_macro_with_ctx<'a>(
    walker: &CstWalker<'a>,
    node: Node<'a>,
    lambda_ctx: &LambdaContext,
) -> LowerResult<Stmt> {
    let span = walker.span(&node);

    let macro_ident = walker.find_child(&node, NodeKind::MacroIdentifier);
    let macro_name = match macro_ident {
        Some(ident) => {
            let text = walker.text(&ident);
            text.trim_start_matches('@').to_string()
        }
        None => {
            return Err(UnsupportedFeature::new(
                UnsupportedFeatureKind::MacroCall,
                span,
            ))
        }
    };

    // Find the macro argument list (tree-sitter) or collect direct children (Pure Rust parser)
    let args_node = walker.find_child(&node, NodeKind::MacroArgumentList);

    // For Pure Rust parser: if no MacroArgumentList, get arguments as direct children
    let direct_args: Vec<Node<'a>> = if args_node.is_none() {
        walker
            .named_children(&node)
            .filter(|child| walker.kind(child) != NodeKind::MacroIdentifier)
            .collect()
    } else {
        vec![]
    };

    // Handle special compiler macros first
    match macro_name.as_str() {
        // @__FILE__ - returns the current file path as a string literal
        "__FILE__" => {
            let file_path = lambda_ctx.get_current_file();
            return Ok(Stmt::Expr {
                expr: Expr::Literal(Literal::Str(file_path), span),
                span,
            });
        }
        // @__DIR__ - returns the directory of the current file as a string literal
        "__DIR__" => {
            let dir_path = lambda_ctx.get_current_dir();
            return Ok(Stmt::Expr {
                expr: Expr::Literal(Literal::Str(dir_path), span),
                span,
            });
        }
        // @__LINE__ - returns the current line number as an integer literal
        "__LINE__" => {
            let line_number = span.start_line as i64;
            return Ok(Stmt::Expr {
                expr: Expr::Literal(Literal::Int(line_number), span),
                span,
            });
        }
        // @__MODULE__ - returns the enclosing module as a module literal. Use the
        // lowering-time module stack (Issue #7919) so it resolves to the calling
        // module inside `module M ... end`, not always `Main`.
        "__MODULE__" => {
            return Ok(Stmt::Expr {
                expr: Expr::Literal(
                    Literal::Module(
                        lambda_ctx
                            .current_module()
                            .unwrap_or_else(|| "Main".to_string()),
                    ),
                    span,
                ),
                span,
            });
        }
        // Statement-position `@sync` must execute its body in the enclosing
        // scope so assignments such as `t = @async ...` update surrounding
        // locals, while still aggregating `@async` exceptions and waiting on
        // scheduled tasks. The expression-position `@sync` path (an isolated
        // `Expr::LetBlock`) is intentionally left unchanged (Issue #7768).
        "sync" => {
            return expr::lower_sync_macro_stmt_entry(walker, node, span, lambda_ctx);
        }
        "threads" => {
            let args = macro_args(walker, node, args_node, &direct_args);
            if args.len() != 1 {
                return Err(
                    UnsupportedFeature::new(UnsupportedFeatureKind::MacroCall, span)
                        .with_hint("@threads requires exactly one for loop"),
                );
            }
            return super::lower_stmt_with_ctx(walker, args[0], lambda_ctx);
        }
        // The remaining task macros are expression-valued, but they are also
        // valid as standalone statements.
        "task" | "async" | "spawn" | "invoke" | "invokelatest" | "views" | "__dot__" | "." => {
            let expr = expr::lower_expr_with_ctx(walker, node, lambda_ctx)?;
            return Ok(Stmt::Expr { expr, span });
        }
        "eval" => {
            let args = macro_args(walker, node, args_node, &direct_args);
            if let Some(stmt) = lower_eval_function_definition(walker, &args, span, lambda_ctx)? {
                return Ok(stmt);
            }
            if let Some(stmt) = lower_eval_module_macro(walker, &args, span)? {
                return Ok(stmt);
            }
        }
        // `Base.@pure` is compiler metadata in upstream Julia. SubsetJuliaVM has
        // no inference purity contract, so statement position preserves the
        // wrapped definition/expression as a compatibility no-op.
        "pure" => {
            let args = macro_args(walker, node, args_node, &direct_args);
            if args.len() != 1 {
                return Err(
                    UnsupportedFeature::new(UnsupportedFeatureKind::MacroCall, span)
                        .with_hint("@pure requires exactly one function definition"),
                );
            }
            return super::lower_stmt_with_ctx(walker, args[0], lambda_ctx);
        }
        "opaque" => {
            return Err(unsupported_opaque_closure_macro(span));
        }
        // Compiler metadata annotations. Upstream Julia records these as Expr(:meta, ...)
        // around definitions or blocks; SubsetJuliaVM supports a compatibility slice:
        // metadata-only annotations pass through, and @boundscheck becomes a runtime guard.
        "inline" | "noinline" | "nospecializeinfer" | "boundscheck" | "propagate_inbounds" => {
            return lower_meta_annotation_compat(
                walker,
                &macro_name,
                node,
                args_node,
                &direct_args,
                span,
                lambda_ctx,
            );
        }
        "constprop" => {
            return lower_constprop_compat(walker, node, args_node, &direct_args, span, lambda_ctx);
        }
        "assume_effects" => {
            return lower_assume_effects_compat(
                walker,
                node,
                args_node,
                &direct_args,
                span,
                lambda_ctx,
            );
        }
        // Statement-position specialization markers such as
        // `@nospecialize x y` and `@specialize` are accepted by upstream Julia
        // as compiler metadata. Retain the marker explicitly in IR while
        // current VM/AoT execution treats it as a no-op.
        "nospecialize" | "specialize" => {
            let marker_args = macro_args(walker, node, args_node, &direct_args)
                .iter()
                .map(|arg| walker.text(arg).to_string())
                .collect();
            return Ok(lower_meta_marker_stmt(&macro_name, marker_args, span));
        }
        // Full `@generated function ... end` syntax. Upstream rewrites this to
        // an `if Expr(:generated)` wrapper and later invokes generated-body
        // machinery. SubsetJuliaVM has no JIT path, so this compatibility slice
        // accepts the definition, unquotes simple returned expressions, and
        // relies on existing type/value parameter binding for bodies that
        // directly return those parameters.
        "generated" => {
            return lower_generated_function_compat(
                walker,
                node,
                args_node,
                &direct_args,
                span,
                lambda_ctx,
            );
        }
        // @label name - define a jump target for @goto
        // Usage: @label myloop
        "label" => {
            // Get the macro arguments
            let args: Vec<Node<'a>> = if let Some(args_node) = args_node {
                walker.named_children_vec(&args_node)
            } else {
                direct_args.clone()
            };

            if args.len() != 1 {
                return Err(
                    UnsupportedFeature::new(UnsupportedFeatureKind::MacroCall, span)
                        .with_hint("@label requires exactly one identifier argument: @label name"),
                );
            }

            // The argument must be an identifier (symbol)
            let arg = args[0];
            let arg_kind = walker.kind(&arg);
            if arg_kind != NodeKind::Identifier {
                return Err(
                    UnsupportedFeature::new(UnsupportedFeatureKind::MacroCall, span)
                        .with_hint("@label argument must be an identifier: @label name"),
                );
            }

            let label_name = walker.text(&arg).to_string();
            return Ok(Stmt::Label {
                name: label_name,
                span,
            });
        }
        // @goto name - unconditionally jump to @label name
        // Usage: @goto myloop
        "goto" => {
            // Get the macro arguments
            let args: Vec<Node<'a>> = if let Some(args_node) = args_node {
                walker.named_children_vec(&args_node)
            } else {
                direct_args.clone()
            };

            if args.len() != 1 {
                return Err(
                    UnsupportedFeature::new(UnsupportedFeatureKind::MacroCall, span)
                        .with_hint("@goto requires exactly one identifier argument: @goto name"),
                );
            }

            // The argument must be an identifier (symbol)
            let arg = args[0];
            let arg_kind = walker.kind(&arg);
            if arg_kind != NodeKind::Identifier {
                return Err(
                    UnsupportedFeature::new(UnsupportedFeatureKind::MacroCall, span)
                        .with_hint("@goto argument must be an identifier: @goto name"),
                );
            }

            let target_name = walker.text(&arg).to_string();
            return Ok(Stmt::Goto {
                name: target_name,
                span,
            });
        }
        // @simd - SIMD loop optimization hint (no-op in SubsetJuliaVM)
        // In SubsetJuliaVM, we don't have JIT or LLVM vectorization, so @simd simply
        // passes through the for loop body unchanged. This allows code that uses @simd
        // to run without modification.
        // Usage: @simd for i in 1:n ... end
        //        @simd ivdep for i in 1:n ... end
        "simd" => {
            // Get the macro arguments
            let args: Vec<Node<'a>> = if let Some(args_node) = args_node {
                walker.named_children_vec(&args_node)
            } else {
                walker
                    .named_children(&node)
                    .filter(|child| walker.kind(child) != NodeKind::MacroIdentifier)
                    .collect()
            };

            if args.is_empty() {
                return Err(
                    UnsupportedFeature::new(UnsupportedFeatureKind::MacroCall, span)
                        .with_hint("@simd requires a for loop argument"),
                );
            }

            // Find the for loop - it's either the first or second argument
            // (second if first is "ivdep")
            let for_loop_node = if args.len() >= 2 {
                // Check if first arg is "ivdep" identifier
                let first_kind = walker.kind(&args[0]);
                if first_kind == NodeKind::Identifier && walker.text(&args[0]) == "ivdep" {
                    // @simd ivdep for ... - use second argument
                    args[1]
                } else {
                    // Use first argument
                    args[0]
                }
            } else {
                // Single argument - must be the for loop
                args[0]
            };

            // Check that it's a for loop
            let for_kind = walker.kind(&for_loop_node);
            if for_kind != NodeKind::ForStatement {
                return Err(
                    UnsupportedFeature::new(UnsupportedFeatureKind::MacroCall, span)
                        .with_hint("@simd requires a for loop as its argument"),
                );
            }

            // Lower the for loop normally - @simd is a no-op in SubsetJuliaVM
            return crate::lowering::stmt::lower_for_stmt_with_ctx(
                walker,
                for_loop_node,
                lambda_ctx,
            );
        }
        // @inbounds - mark the wrapped expression as an inbounds call context.
        // Usage: @inbounds expr
        //        @inbounds for i in 1:n ... end
        "inbounds" => {
            // Get the macro arguments
            let args: Vec<Node<'a>> = if let Some(args_node) = args_node {
                walker.named_children_vec(&args_node)
            } else {
                walker
                    .named_children(&node)
                    .filter(|child| walker.kind(child) != NodeKind::MacroIdentifier)
                    .collect()
            };

            if args.is_empty() {
                return Err(
                    UnsupportedFeature::new(UnsupportedFeatureKind::MacroCall, span)
                        .with_hint("@inbounds requires an argument"),
                );
            }

            // Lower the argument as-is
            let arg = args[0];
            let arg_kind = walker.kind(&arg);

            // Handle different statement types
            return match arg_kind {
                NodeKind::ForStatement => {
                    crate::lowering::stmt::lower_for_stmt_with_ctx(walker, arg, lambda_ctx)
                        .map(mark_stmt_inbounds)
                }
                NodeKind::WhileStatement => {
                    crate::lowering::stmt::lower_while_stmt_with_ctx(walker, arg, lambda_ctx)
                        .map(mark_stmt_inbounds)
                }
                NodeKind::IfStatement => {
                    crate::lowering::stmt::lower_if_stmt_with_ctx(walker, arg, lambda_ctx)
                        .map(mark_stmt_inbounds)
                }
                NodeKind::Assignment => {
                    if let Some(stmt) =
                        lower_inbounds_index_assignment(walker, arg, span, lambda_ctx)?
                    {
                        return Ok(stmt);
                    }
                    super::lower_stmt_with_ctx(walker, arg, lambda_ctx).map(mark_stmt_inbounds)
                }
                NodeKind::TupleExpression => {
                    if let Some(stmt) =
                        lower_inbounds_tuple_index_assignment(walker, arg, span, lambda_ctx)?
                    {
                        return Ok(stmt);
                    }
                    let expr_result = expr::lower_expr_with_ctx(walker, arg, lambda_ctx)?;
                    let expr_result = Expr::Call {
                        function: "#__sjulia_inbounds__".to_string().into(),
                        args: vec![expr_result],
                        kwargs: vec![],
                        splat_mask: vec![],
                        kwargs_splat_mask: vec![],
                        span,
                    };
                    Ok(Stmt::Expr {
                        expr: expr_result,
                        span,
                    })
                }
                _ => {
                    let expr_result = expr::lower_expr_with_ctx(walker, arg, lambda_ctx)?;
                    let expr_result = Expr::Call {
                        function: "#__sjulia_inbounds__".to_string().into(),
                        args: vec![expr_result],
                        kwargs: vec![],
                        splat_mask: vec![],
                        kwargs_splat_mask: vec![],
                        span,
                    };
                    Ok(Stmt::Expr {
                        expr: expr_result,
                        span,
                    })
                }
            };
        }
        // @view A[i:j] - create a view of an array slice
        // Transforms A[indices...] into view(A, indices...)
        "view" => {
            // Get the macro arguments
            let args: Vec<Node<'a>> = if let Some(args_node) = args_node {
                walker.named_children_vec(&args_node)
            } else if !direct_args.is_empty() {
                direct_args.to_vec()
            } else {
                vec![]
            };

            if args.len() != 1 {
                return Err(
                    UnsupportedFeature::new(UnsupportedFeatureKind::MacroCall, span)
                        .with_hint("@view requires exactly one argument: @view A[indices...]"),
                );
            }

            let arg = args[0];
            let arg_kind = walker.kind(&arg);

            // Check if the argument is an indexing expression (index_expression/ref)
            if arg_kind == NodeKind::IndexExpression {
                // Get the array and indices from the index expression
                // IndexExpression has children: [array, indices...]
                let sub_children: Vec<Node<'a>> = walker.named_children_vec(&arg);
                if sub_children.is_empty() {
                    return Err(
                        UnsupportedFeature::new(UnsupportedFeatureKind::MacroCall, span).with_hint(
                            "@view requires an indexing expression: @view A[indices...]",
                        ),
                    );
                }

                // First child is the array
                let array_node = sub_children[0];
                let array_expr = expr::lower_expr_with_ctx(walker, array_node, lambda_ctx)?;

                // Remaining children are the indices
                let index_exprs: Vec<Expr> = sub_children[1..]
                    .iter()
                    .map(|idx| expr::lower_expr_with_ctx(walker, *idx, lambda_ctx))
                    .collect::<Result<Vec<_>, _>>()?;

                // Build the view function call: view(array, indices...)
                let mut call_args = vec![array_expr];
                call_args.extend(index_exprs);

                return Ok(Stmt::Expr {
                    expr: Expr::Call {
                        function: "view".to_string().into(),
                        args: call_args,
                        kwargs: vec![],
                        splat_mask: vec![],
                        kwargs_splat_mask: vec![],
                        span,
                    },
                    span,
                });
            } else {
                // Not an indexing expression - return error
                return Err(
                    UnsupportedFeature::new(UnsupportedFeatureKind::MacroCall, span)
                        .with_hint("@view requires an indexing expression: @view A[indices...]"),
                );
            }
        }
        // @code_warntype f(args...) - type-stability diagnostic (Issue #5145).
        // Mirrors upstream InteractiveUtils: evaluate the call arguments, take
        // their types, and forward to `code_warntype(f, Tuple{typeof(args)...})`.
        // Gated on `using InteractiveUtils`, matching where the macro is exported.
        "code_warntype" => {
            if !lambda_ctx
                .get_usings()
                .iter()
                .any(|u| u == "InteractiveUtils")
            {
                return Err(
                    UnsupportedFeature::new(UnsupportedFeatureKind::MacroCall, span)
                        .with_hint("@code_warntype macro requires `using InteractiveUtils`"),
                );
            }
            let args = macro_args(walker, node, args_node, &direct_args);
            let expr = lower_code_warntype_macro(walker, &args, span, lambda_ctx)?;
            return Ok(Stmt::Expr { expr, span });
        }
        // @NamedTuple{a::Int, b} / @NamedTuple begin ... end -> NamedTuple type (Issue #5120)
        "NamedTuple" => {
            let args = macro_args(walker, node, args_node, &direct_args);
            let expr = crate::lowering::expr::lower_namedtuple_macro_expr(walker, &args, span)?;
            return Ok(Stmt::Expr { expr, span });
        }
        // @static - compile-time conditional evaluation
        // Usage: @static if cond ... else ... end
        //        @static cond ? a : b
        // Evaluates the condition at compile time and only includes the selected branch.
        // Primarily used with Sys.isapple(), Sys.isunix(), Sys.iswindows(), Sys.islinux()
        "static" => {
            return lower_static_macro_with_ctx(
                walker,
                node,
                args_node,
                &direct_args,
                span,
                lambda_ctx,
            );
        }
        // @enum - define an enumerated type
        // Usage: @enum TypeName member1 member2 member3
        //        @enum TypeName member1=1 member2=2 member3=10
        //        @enum TypeName::BaseType member1 member2
        // Creates named constants backed by integers
        "enum" => {
            return lower_enum_macro_with_ctx(
                walker,
                node,
                args_node,
                &direct_args,
                span,
                lambda_ctx,
            );
        }
        // @irrational sym def - define an Irrational{sym} singleton constant.
        // The VM supplies numeric constructors for the well-known constants
        // used by Base.MathConstants (Issue #5133).
        "irrational" => {
            return lower_irrational_macro(walker, node, args_node, &direct_args, span, lambda_ctx);
        }
        // @isdefined(x) - check if variable x is defined
        // Returns a Builtin expression that will be compiled to IsDefined instruction
        "isdefined" => {
            // Get the argument (variable name)
            let args: Vec<Node<'a>> = if let Some(args_node) = args_node {
                walker.named_children_vec(&args_node)
            } else if !direct_args.is_empty() {
                direct_args.to_vec()
            } else {
                vec![]
            };

            if args.len() != 1 {
                return Err(
                    UnsupportedFeature::new(UnsupportedFeatureKind::MacroCall, span)
                        .with_hint("@isdefined requires exactly one argument (a variable name)"),
                );
            }

            // Get the variable name from the argument
            // It should be an identifier (variable reference)
            let arg = args[0];
            let arg_kind = walker.kind(&arg);
            let var_name = match arg_kind {
                NodeKind::Identifier => walker.text(&arg).to_string(),
                _ => {
                    return Err(
                        UnsupportedFeature::new(UnsupportedFeatureKind::MacroCall, span)
                            .with_hint("@isdefined argument must be a variable name (identifier)"),
                    );
                }
            };

            // Return a Builtin expression that the compiler will convert to IsDefined instruction
            return Ok(Stmt::Expr {
                expr: Expr::Builtin {
                    name: crate::ir::core::BuiltinOp::IsDefined,
                    args: vec![Expr::Literal(Literal::Str(var_name), span)],
                    span,
                },
                span,
            });
        }
        _ => {}
    }

    // All macros are handled via the macro registry (user-defined, Base, or stdlib)
    // No more hardcoded macro handlers - everything goes through Pure Julia
    if lambda_ctx.has_macro(&macro_name) {
        // Check for user-defined macro (local context first)
        expand_user_defined_macro(
            walker,
            node,
            &macro_name,
            args_node,
            &direct_args,
            span,
            lambda_ctx,
        )
    } else if crate::lowering::macros_registry::has_base_macro(&macro_name) {
        // Check for Base macro (from base/macros.jl)
        expand_base_macro(
            walker,
            node,
            &macro_name,
            args_node,
            &direct_args,
            span,
            lambda_ctx,
        )
    } else if let Some(module_name) = lambda_ctx
        .get_usings()
        .iter()
        .find(|m| crate::lowering::macros_registry::has_bundled_package_macro(m, &macro_name))
        .cloned()
    {
        // Bundled-package macro in statement position, e.g. a standalone
        // `@gif for … end` (Issue #6355). Expanded through the full macro_runtime
        // path, like user-defined macros.
        expand_bundled_package_macro(
            walker,
            &module_name,
            &macro_name,
            args_node,
            &direct_args,
            span,
            lambda_ctx,
        )
    } else {
        // Check for stdlib macros from imported modules
        let mut found_macro = None;
        for module_name in lambda_ctx.get_usings() {
            if crate::lowering::macros_registry::has_stdlib_macro(&module_name, &macro_name) {
                found_macro = Some(module_name);
                break;
            }
        }
        if let Some(module_name) = found_macro {
            expand_stdlib_macro(
                walker,
                node,
                &module_name,
                &macro_name,
                args_node,
                &direct_args,
                span,
                lambda_ctx,
            )
        } else if macro_name == "test"
            || macro_name == "testset"
            || macro_name == "test_throws"
            || macro_name == "test_broken"
            || macro_name == "test_skip"
        {
            // Special case: @test, @testset, @test_throws, @test_broken,
            // @test_skip require using Test
            Err(
                UnsupportedFeature::new(UnsupportedFeatureKind::MacroCall, span)
                    .with_hint(format!("@{} macro requires `using Test`", macro_name)),
            )
        } else {
            Err(
                UnsupportedFeature::new(UnsupportedFeatureKind::MacroCall, span)
                    .with_hint(format!("unknown macro @{}", macro_name)),
            )
        }
    }
}

#[cfg(test)]
#[allow(clippy::unwrap_used, clippy::expect_used)]
mod tests {
    use crate::lowering::Lowering;
    use crate::parser::Parser;

    fn lower_error(source: &str) -> String {
        let mut parser = Parser::new().expect("parser");
        let parse_outcome = parser.parse(source).expect("parse");
        let mut lowering = Lowering::new(source);
        let err = lowering
            .lower(parse_outcome)
            .expect_err("lowering should fail");
        err.to_string()
    }

    #[test]
    fn qualified_opaque_macro_reports_explicit_unsupported_statement_diagnostic() {
        let err = lower_error("Base.Experimental.@opaque x -> x + 1");
        assert!(err.contains("opaque closures"), "{err}");
        assert!(err.contains("Issue #4289"), "{err}");
        assert!(!err.contains("unknown macro @opaque"), "{err}");
    }
}
