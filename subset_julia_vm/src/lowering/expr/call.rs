//! Function call expression lowering.
//!
//! This module handles lowering of function calls, arrow functions,
//! do syntax, and argument list parsing.

use crate::error::{UnsupportedFeature, UnsupportedFeatureKind};
use crate::ir::core::{BinaryOp, Block, Expr, Function, Stmt, TypedParam, UnaryOp};
use crate::lowering::stmt::lower_stmt;
use crate::lowering::{LambdaContext, LowerResult};
use crate::parser::cst::{CstWalker, Node, NodeKind};
use crate::stdlib;

use super::{is_broadcast_op, lower_expr, lower_expr_with_ctx, map_builtin_name};

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
            let children = walker.named_children(&callee);
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

/// Extract the inner `ArrowFunctionExpression` from a `ParenthesizedExpression` callee,
/// for handling immediately invoked lambda expressions: `(x -> expr)(args)` (Issue #3142).
///
/// Returns `Some(arrow_node)` if the callee is `(arrow_function)`, `None` otherwise.
fn extract_paren_arrow_function<'a>(walker: &CstWalker<'a>, callee: Node<'a>) -> Option<Node<'a>> {
    if walker.kind(&callee) != NodeKind::ParenthesizedExpression {
        return None;
    }
    let children = walker.named_children(&callee);
    if children.len() == 1 && walker.kind(&children[0]) == NodeKind::ArrowFunctionExpression {
        Some(children[0])
    } else {
        None
    }
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
///     Stmt::Return(Call(__iife_N, args)),  // calls it
///   ]
/// }
/// ```
fn lower_iife_as_nested<'a>(
    walker: &CstWalker<'a>,
    arrow_node: Node<'a>,
    args: Vec<Expr>,
    kwargs: Vec<(String, Expr)>,
    splat_mask: Vec<bool>,
    kwargs_splat_mask: Vec<bool>,
    span: crate::span::Span,
) -> LowerResult<Expr> {
    let arrow_span = walker.span(&arrow_node);
    let children = walker.named_children(&arrow_node);
    let child_count = children.len();

    let mut params: Vec<TypedParam> = Vec::new();
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
                }
                NodeKind::ArgumentList
                | NodeKind::TupleExpression
                | NodeKind::ParenthesizedExpression => {
                    for arg in walker.named_children(child) {
                        if walker.kind(&arg) == NodeKind::Identifier {
                            let name = walker.text(&arg).to_string();
                            params.push(TypedParam::untyped(name, walker.span(&arg)));
                        }
                    }
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
        kwparams: vec![],
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
        span: arrow_span,
    };

    let func_def_stmt = Stmt::FunctionDef {
        func: Box::new(func),
        span,
    };

    let call_expr = Expr::Call {
        function: iife_name,
        args,
        kwargs,
        splat_mask,
        kwargs_splat_mask,
        span,
    };

    Ok(Expr::LetBlock {
        bindings: vec![],
        body: Block {
            stmts: vec![
                func_def_stmt,
                Stmt::Return {
                    value: Some(call_expr),
                    span,
                },
            ],
            span,
        },
        span,
    })
}

/// Generate a closure for operator partial application: `op(x)` → `y -> y op x` (Issue #3119).
///
/// `op_text` is the operator string (e.g. "==", ">").
/// `arg_expr` is the already-lowered right-hand argument (inlined into the lambda body).
/// The argument expression is inlined directly so that free variable analysis can detect
/// captures from the enclosing function's params (e.g. `==(n)` inside `foo(n)` captures `n`).
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

    // Lambda body: __op_y op arg_expr
    // arg_expr is inlined so that free variables in arg_expr are correctly detected
    // as captures when the lambda is analyzed against the enclosing function's scope.
    let body_expr = Expr::BinaryOp {
        op: bin_op,
        left: Box::new(Expr::Var(param_name.clone(), span)),
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
        span,
    };

    lambda_ctx.add_lifted_function(func);

    Ok(Expr::FunctionRef {
        name: lambda_name,
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
        left: Box::new(Expr::Var(param_name.clone(), span)),
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
        span,
    };

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
        expr: Expr::Var(lambda_name.clone(), span),
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
            name: walker.text(&callee).to_string(),
        }),
        NodeKind::ParametrizedTypeExpression => Ok(ResolvedCallTarget::DirectCall {
            name: walker.text(&callee).to_string(),
        }),
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
                    let arg_children = walker.named_children(args);
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
            if is_broadcast_op(&op_text) {
                Ok(ResolvedCallTarget::DirectCall { name: op_text })
            } else {
                Err(UnsupportedFeature::new(
                    UnsupportedFeatureKind::UnsupportedCallTarget,
                    walker.span(&callee),
                ))
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
    kwargs: Vec<(String, Expr)>,
    splat_mask: Vec<bool>,
    kwargs_splat_mask: Vec<bool>,
    span: crate::span::Span,
) -> Expr {
    let call_expr = Expr::Call {
        function: temp_name.clone(),
        args,
        kwargs,
        splat_mask,
        kwargs_splat_mask,
        span,
    };

    Expr::LetBlock {
        bindings: vec![(temp_name, callee_expr)],
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
        let arg_children = walker.named_children(&arg_node);
        if let Some(first_arg) = arg_children.first() {
            if walker.kind(first_arg) == NodeKind::StringLiteral {
                let text = walker.text(first_arg);
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
    let children = walker.named_children(&node);

    // Find parameters and body
    // Structure: identifier/argument_list, ->, expression
    // Note: named_children excludes the -> operator (not a named node),
    // so named children are [params..., body]. The LAST child is always the body.
    let mut params: Vec<TypedParam> = Vec::new();
    let mut body_expr: Option<Expr> = None;
    let child_count = children.len();

    for (i, child) in children.iter().enumerate() {
        let is_last = i == child_count - 1;

        if is_last {
            // Last named child is always the body expression
            body_expr = Some(lower_expr(walker, *child)?);
        } else {
            match walker.kind(child) {
                NodeKind::Identifier => {
                    // Parameter before ->
                    let name = walker.text(child).to_string();
                    params.push(TypedParam::untyped(name, walker.span(child)));
                }
                NodeKind::ArgumentList
                | NodeKind::TupleExpression
                | NodeKind::ParenthesizedExpression => {
                    // Multiple parameters: (x, y) -> ...
                    for arg in walker.named_children(child) {
                        if walker.kind(&arg) == NodeKind::Identifier {
                            let name = walker.text(&arg).to_string();
                            params.push(TypedParam::untyped(name, walker.span(&arg)));
                        }
                    }
                }
                _ => {
                    // Unexpected node before body
                }
            }
        }
    }

    let body_expr = body_expr.ok_or_else(|| {
        UnsupportedFeature::new(
            UnsupportedFeatureKind::UnsupportedExpression("lambda without body".to_string()),
            span,
        )
    })?;

    // Create a function with the body wrapped in a return statement
    let lambda_name = lambda_ctx.next_lambda_name();
    let func = Function {
        name: lambda_name.clone(),
        params,
        kwparams: vec![],
        type_params: Vec::new(),
        return_type: None,
        body: Block {
            stmts: vec![Stmt::Return {
                value: Some(body_expr),
                span,
            }],
            span,
        },
        is_base_extension: false,
        span,
    };

    // Add to lifted functions
    lambda_ctx.add_lifted_function(func);

    // Return a FunctionRef pointing to the lifted function
    Ok(Expr::FunctionRef {
        name: lambda_name,
        span,
    })
}

/// Lower call expression with lambda context (handles do syntax)
pub fn lower_call_expr_with_ctx<'a>(
    walker: &CstWalker<'a>,
    node: Node<'a>,
    lambda_ctx: &LambdaContext,
) -> LowerResult<Expr> {
    let span = walker.span(&node);
    let named = walker.named_children(&node);

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
            .into_iter()
            .find(|n| walker.kind(n) == NodeKind::DoClause)
    });

    // Check for operator partial application: ==(x), >(3), (!=)(val), etc. (Issue #3119)
    // Transform op(x) → closure y -> y op x using a lambda lifted to the current scope.
    if let Some(op) = extract_partial_apply_operator(walker, callee) {
        let (args, _, _, _) = match args_node {
            Some(node) => lower_argument_list_with_ctx(walker, node, lambda_ctx)?,
            None => (Vec::new(), Vec::new(), Vec::new(), Vec::new()),
        };
        if args.len() == 1 {
            return lower_operator_partial_apply(
                &op,
                args.into_iter().next().unwrap(),
                lambda_ctx,
                span,
            );
        }
    }

    // Handle immediately invoked lambda: (x -> expr)(args) (Issue #3142)
    // When the callee is a parenthesized arrow function literal, lift the lambda and call it.
    if let Some(arrow_node) = extract_paren_arrow_function(walker, callee) {
        let lambda_ref = lower_arrow_function(walker, arrow_node, lambda_ctx)?;
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

    // Resolve call target using shared helper (Issue #2271)
    match resolve_call_target(walker, callee, &named, span)? {
        ResolvedCallTarget::ModuleCall { module, function } => {
            let (args, kwargs, _splat_mask, _kwargs_splat_mask) = match args_node {
                Some(node) => lower_argument_list_with_ctx(walker, node, lambda_ctx)?,
                None => (Vec::new(), Vec::new(), Vec::new(), Vec::new()),
            };
            Ok(Expr::ModuleCall {
                module,
                function,
                args,
                kwargs,
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
        ResolvedCallTarget::UnaryNot { operand } => {
            Ok(Expr::UnaryOp {
                op: UnaryOp::Not,
                operand: Box::new(operand),
                span,
            })
        }
        ResolvedCallTarget::DirectCall { name } => {
            // Special handling for include("path") - file inclusion
            if name == "include" {
                let path = extract_include_path(walker, args_node);
                return Err(UnsupportedFeature::new(
                    UnsupportedFeatureKind::IncludeCall(path),
                    span,
                ).with_hint("include is not supported in sandboxed environments. Use prelude for bundled functions, or define functions directly in the source."));
            }

            // Handle do clause: map([1,2,3]) do x; x^2 end
            if let Some(do_node) = do_clause {
                let lambda_ref = lower_do_clause(walker, do_node, lambda_ctx)?;

                // Get regular arguments (excluding do_clause)
                let (regular_args, _kwargs, _splat_mask, _kwargs_splat_mask) = match args_node {
                    Some(node) => lower_argument_list_with_kwargs(walker, node)?,
                    None => (Vec::new(), Vec::new(), Vec::new(), Vec::new()),
                };

                // For do syntax: function(lambda_ref, regular_args...)
                let mut all_args = vec![lambda_ref];
                all_args.extend(regular_args);

                return Ok(Expr::Call {
                    function: name,
                    args: all_args,
                    kwargs: vec![],
                    splat_mask: vec![],
                    kwargs_splat_mask: vec![],
                    span,
                });
            }

            // Regular call - check if any argument is an arrow function
            let (args, kwargs, splat_mask, kwargs_splat_mask) = match args_node {
                Some(arg_node) => lower_argument_list_with_ctx(walker, arg_node, lambda_ctx)?,
                None => (Vec::new(), Vec::new(), Vec::new(), Vec::new()),
            };

            // Special handling for range(start, stop; length=N) -> range(start, stop, N)
            if name == "range" && args.len() == 2 && !kwargs.is_empty() {
                for (key, value) in &kwargs {
                    if key == "length" {
                        let mut positional_args = args.clone();
                        positional_args.push(value.clone());
                        return Ok(Expr::Call {
                            function: name,
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
                return Ok(Expr::Builtin {
                    name: builtin,
                    args,
                    span,
                });
            }

            Ok(Expr::Call {
                function: name,
                args,
                kwargs,
                splat_mask,
                kwargs_splat_mask,
                span,
            })
        }
    }
}

/// Lower do clause to a FunctionRef
fn lower_do_clause<'a>(
    walker: &CstWalker<'a>,
    node: Node<'a>,
    lambda_ctx: &LambdaContext,
) -> LowerResult<Expr> {
    let span = walker.span(&node);
    let children = walker.named_children(&node);

    // Structure: do, ParameterList/identifier(s), block, end
    // Pure Rust parser wraps params in ParameterList, tree-sitter uses direct identifiers
    let mut params: Vec<TypedParam> = Vec::new();
    let mut body_block: Option<Block> = None;

    for child in children {
        match walker.kind(&child) {
            NodeKind::ParameterList => {
                // Pure Rust parser: parameters wrapped in ParameterList
                for param in walker.named_children(&child) {
                    if walker.kind(&param) == NodeKind::Identifier {
                        let name = walker.text(&param).to_string();
                        params.push(TypedParam::untyped(name, walker.span(&param)));
                    }
                }
            }
            NodeKind::Identifier => {
                // Tree-sitter style: direct identifiers
                let name = walker.text(&child).to_string();
                params.push(TypedParam::untyped(name, walker.span(&child)));
            }
            NodeKind::Block => {
                body_block = Some(lower_block_simple(walker, child)?);
            }
            _ => {}
        }
    }

    let body = body_block.unwrap_or(Block {
        stmts: vec![],
        span,
    });

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
        span,
    };

    lambda_ctx.add_lifted_function(func);

    Ok(Expr::FunctionRef {
        name: lambda_name,
        span,
    })
}

/// Simple block lowering without lambda context (for do blocks)
fn lower_block_simple<'a>(walker: &CstWalker<'a>, node: Node<'a>) -> LowerResult<Block> {
    let span = walker.span(&node);
    let mut stmts = Vec::new();

    let children: Vec<_> = walker.named_children(&node);
    let last_id = children.last().map(|n| n.id());

    for child in children {
        // For do blocks, we need to wrap the last expression as a return
        let is_last = Some(child.id()) == last_id;
        let child_span = walker.span(&child);

        match walker.kind(&child) {
            NodeKind::ReturnStatement => {
                let value = walker
                    .named_children(&child)
                    .pop()
                    .map(|n| lower_expr(walker, n))
                    .transpose()?;
                stmts.push(Stmt::Return {
                    value,
                    span: child_span,
                });
            }
            NodeKind::Assignment => {
                // Handle assignment statements properly
                let stmt = lower_stmt(walker, child)?;
                if is_last {
                    // If the last statement is an assignment, we need to return the assigned value
                    // Extract the variable name from the assignment and return it
                    if let Stmt::Assign { var, .. } = &stmt {
                        let var_name = var.clone();
                        stmts.push(stmt);
                        stmts.push(Stmt::Return {
                            value: Some(Expr::Var(var_name, child_span)),
                            span: child_span,
                        });
                    } else {
                        stmts.push(stmt);
                    }
                } else {
                    stmts.push(stmt);
                }
            }
            _ => {
                let expr = lower_expr(walker, child)?;
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

/// Lower argument list with lambda context (handles arrow functions as arguments)
/// Returns (positional_args, keyword_args, splat_mask, kwargs_splat_mask)
fn lower_argument_list_with_ctx<'a>(
    walker: &CstWalker<'a>,
    node: Node<'a>,
    lambda_ctx: &LambdaContext,
) -> LowerResult<(Vec<Expr>, Vec<(String, Expr)>, Vec<bool>, Vec<bool>)> {
    let children = walker.named_children(&node);

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
                let expr = lower_arrow_function(walker, child, lambda_ctx)?;
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
                let inner_children: Vec<_> = walker.named_children(&child);
                if let Some(inner) = inner_children.first() {
                    // Use empty string as key to mark this as a splat
                    kwargs.push((
                        "".to_string(),
                        lower_expr_with_ctx(walker, *inner, lambda_ctx)?,
                    ));
                    kwargs_splat_mask.push(true);
                }
            }
            NodeKind::SplatExpression => {
                // Positional splat expression: args... - extract inner expression and mark for runtime expansion
                let inner_children: Vec<_> = walker.named_children(&child);
                if let Some(inner) = inner_children.first() {
                    positional_args.push(lower_expr_with_ctx(walker, *inner, lambda_ctx)?);
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
                    name: op_text,
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
    let named = walker.named_children(&node);
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
            return lower_operator_partial_apply_as_nested(
                &op,
                args.into_iter().next().unwrap(),
                span,
            );
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
        return lower_iife_as_nested(walker, arrow_node, args, kwargs, splat_mask, kwargs_splat_mask, span);
    }

    // Resolve call target using shared helper (Issue #2271)
    match resolve_call_target(walker, callee, &named, span)? {
        ResolvedCallTarget::ModuleCall { module, function } => {
            let args_node = find_args_node(walker, &named);
            let (args, kwargs, _splat_mask, _kwargs_splat_mask) = match args_node {
                Some(node) => lower_argument_list_with_kwargs(walker, node)?,
                None => (Vec::new(), Vec::new(), Vec::new(), Vec::new()),
            };
            Ok(Expr::ModuleCall {
                module,
                function,
                args,
                kwargs,
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
        ResolvedCallTarget::UnaryNot { operand } => {
            Ok(Expr::UnaryOp {
                op: UnaryOp::Not,
                operand: Box::new(operand),
                span,
            })
        }
        ResolvedCallTarget::DirectCall { name } => {
            let args_node = find_args_node(walker, &named);

            // Special handling for include("path") - file inclusion
            if name == "include" {
                let path = extract_include_path(walker, args_node);
                return Err(UnsupportedFeature::new(
                    UnsupportedFeatureKind::IncludeCall(path),
                    span,
                ).with_hint("include is not supported in sandboxed environments. Use prelude for bundled functions, or define functions directly in the source."));
            }

            // Handle new() and new{T}() for inner constructors
            if name == "new" || name.starts_with("new{") {
                let type_args: Vec<crate::types::TypeExpr> =
                    if name.starts_with("new{") && name.ends_with('}') {
                        let type_args_str = &name[4..name.len() - 1];
                        type_args_str
                            .split(',')
                            .map(|s| crate::types::TypeExpr::TypeVar(s.trim().to_string()))
                            .collect()
                    } else {
                        vec![]
                    };

                let is_splat = if let Some(args_node) = args_node {
                    let children: Vec<_> = walker
                        .named_children(&args_node)
                        .into_iter()
                        .filter(|n| walker.kind(n) != NodeKind::Operator)
                        .collect();
                    if let Some(last) = children.last() {
                        walker.kind(last) == NodeKind::SplatExpression
                    } else {
                        false
                    }
                } else {
                    false
                };

                let args = if is_splat {
                    if let Some(args_node) = args_node {
                        let children: Vec<_> = walker
                            .named_children(&args_node)
                            .into_iter()
                            .filter(|n| walker.kind(n) != NodeKind::Operator)
                            .collect();
                        if let Some(splat_node) = children.last() {
                            let inner = walker.named_children(splat_node);
                            if let Some(inner_node) = inner.first() {
                                vec![lower_expr(walker, *inner_node)?]
                            } else {
                                vec![]
                            }
                        } else {
                            vec![]
                        }
                    } else {
                        vec![]
                    }
                } else {
                    match args_node {
                        Some(node) => lower_argument_list_with_kwargs(walker, node)?.0,
                        None => Vec::new(),
                    }
                };

                return Ok(Expr::New {
                    type_args,
                    args,
                    is_splat,
                    span,
                });
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
                return Ok(Expr::Builtin {
                    name: builtin,
                    args,
                    span,
                });
            }

            Ok(Expr::Call {
                function: name,
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
    if matches!(name, "Base" | "Core" | "Main" | "Pkg") {
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
    let named = walker.named_children(&node);
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
        _ => {
            // For other node kinds, check if text starts with ':'
            // This handles quote expressions like :+, :-, etc.
            if let Some(stripped) = func_text.strip_prefix(':') {
                stripped.to_string()
            } else {
                return None;
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
    let named = walker.named_children(&node);
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
) -> LowerResult<(Vec<Expr>, Vec<(String, Expr)>, Vec<bool>, Vec<bool>)> {
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
                    let inner_children: Vec<_> = walker.named_children(&child);
                    if let Some(inner) = inner_children.first() {
                        // Use empty string as key to mark this as a splat
                        kwargs.push(("".to_string(), lower_expr(walker, *inner)?));
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
                    let inner_children: Vec<_> = walker.named_children(&child);
                    if let Some(inner) = inner_children.first() {
                        positional_args.push(lower_expr(walker, *inner)?);
                        splat_mask.push(true);
                    }
                }
                NodeKind::Operator => {
                    // Bare operator as function argument: map(+, ...), reduce(*, ...)
                    // Convert to FunctionRef so it can be passed as a first-class function (Issue #1985)
                    let op_text = walker.text(&child).to_string();
                    let op_span = walker.span(&child);
                    positional_args.push(Expr::FunctionRef {
                        name: op_text,
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
) -> LowerResult<Option<(String, Expr)>> {
    // Filter out operator nodes to get [name, value]
    let children: Vec<_> = walker
        .named_children(&node)
        .into_iter()
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

    Ok(Some((name, value)))
}

/// Parse a KeywordArgument node (Pure Rust parser format).
/// Structure: KeywordArgument { Identifier, value_expr }
/// When lambda_ctx is provided, arrow functions in kwarg values are supported (Issue #2073).
fn parse_keyword_argument<'a>(
    walker: &CstWalker<'a>,
    node: Node<'a>,
    lambda_ctx: Option<&LambdaContext>,
) -> LowerResult<Option<(String, Expr)>> {
    let children = walker.named_children(&node);

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

    Ok(Some((name, value)))
}
