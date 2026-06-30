//! Statement lowering module
//!
//! This module handles the conversion of CST statement nodes to Core IR statements.
//!
//! ## Submodules
//!
//! - `assignment`: Variable and array assignments, compound assignments
//! - `control_for`: For loop statements
//! - `control_if`: If/elseif/else statements
//! - `control_while`: While loop statements
//! - `control_try`: Try/catch/finally statements
//! - `macros`: Macro calls (@show, @assert, @time, etc.)

mod assignment;
mod control_for;
mod control_if;
mod control_try;
mod control_while;
pub(crate) mod macros;

use crate::error::{UnsupportedFeature, UnsupportedFeatureKind};
use crate::ir::core::{Block, BuiltinOp, Expr, Function, Stmt};
use crate::lowering::expr;
use crate::lowering::function;
use crate::lowering::{contains_macro_call, LambdaContext, LowerResult};
use crate::parser::cst::{CstWalker, Node, NodeKind};
use crate::span::Span;

// Re-export public functions
pub use assignment::{
    lower_assignment, lower_assignment_value_expr, lower_assignment_value_expr_with_ctx,
    lower_assignment_with_ctx, lower_compound_assignment, lower_compound_assignment_expr,
    lower_compound_assignment_expr_with_ctx, lower_compound_assignment_with_ctx,
};
// Tuple-destructuring helpers reused by the macro-expansion lowering path
// (Issue #7900), where assignments arrive as `Expr` values, not CST nodes.
pub(crate) use assignment::{
    lower_destructuring_from_targets, parse_destructure_targets, DestructureTarget,
};
pub(crate) use control_for::lower_for_stmt_with_body;
pub use control_for::{lower_for_stmt, lower_for_stmt_with_ctx};
pub use control_if::{lower_if_stmt, lower_if_stmt_with_ctx};
pub use control_try::{lower_try_stmt, lower_try_stmt_with_ctx};
pub use control_while::{lower_while_stmt, lower_while_stmt_with_ctx};
pub use macros::{lower_macro, lower_macro_with_ctx};

/// Convert a Vec<Function> into a single Stmt.
fn lower_function_defs_to_stmt(mut funcs: Vec<Function>, span: crate::span::Span) -> Stmt {
    if funcs.len() == 1 {
        if let Some(func) = funcs.pop() {
            Stmt::FunctionDef {
                func: Box::new(func),
                span,
            }
        } else {
            Stmt::Block(Block {
                stmts: Vec::new(),
                span,
            })
        }
    } else {
        let stmts = funcs
            .into_iter()
            .map(|f| {
                let s = f.span;
                Stmt::FunctionDef {
                    func: Box::new(f),
                    span: s,
                }
            })
            .collect();
        Stmt::Block(Block { stmts, span })
    }
}

/// Lower a statement node to Core IR.
pub fn lower_stmt<'a>(walker: &CstWalker<'a>, node: Node<'a>) -> LowerResult<Stmt> {
    lower_stmt_impl(walker, node, None)
}

fn lower_stmt_impl<'a>(
    walker: &CstWalker<'a>,
    node: Node<'a>,
    lambda_ctx: Option<&LambdaContext>,
) -> LowerResult<Stmt> {
    let span = walker.span(&node);
    match walker.kind(&node) {
        // Assignment can also be a short function definition (f(x) = 2x) - check this first
        NodeKind::Assignment if function::is_short_function_definition(walker, node) => {
            let funcs = match lambda_ctx {
                Some(ctx) if contains_macro_call(walker, node) => {
                    function::lower_short_function_all_with_ctx(walker, node, ctx)?
                }
                _ => function::lower_short_function_all(walker, node)?,
            };
            Ok(lower_function_defs_to_stmt(funcs, span))
        }
        NodeKind::Assignment => lower_assignment_maybe_ctx(walker, node, lambda_ctx),
        NodeKind::CompoundAssignment => {
            lower_compound_assignment_maybe_ctx(walker, node, lambda_ctx)
        }
        NodeKind::ReturnStatement => lower_return(walker, node, lambda_ctx),
        NodeKind::ForStatement => lower_for_stmt_maybe_ctx(walker, node, lambda_ctx),
        NodeKind::IfStatement => lower_if_stmt_maybe_ctx(walker, node, lambda_ctx),
        NodeKind::WhileStatement => lower_while_stmt_maybe_ctx(walker, node, lambda_ctx),
        NodeKind::BreakStatement => Ok(Stmt::Break { span }),
        NodeKind::ContinueStatement => Ok(Stmt::Continue { span }),
        NodeKind::TryStatement => lower_try_stmt_maybe_ctx(walker, node, lambda_ctx),
        NodeKind::MacroCall => lower_macro_maybe_ctx(walker, node, lambda_ctx),
        // const x = 1 -> treat as regular assignment (simplified implementation)
        NodeKind::ConstStatement => lower_const_statement(walker, node, lambda_ctx),
        // global x -> ignored unless an assignment is present.
        NodeKind::GlobalStatement => lower_global_statement(walker, node, lambda_ctx),
        // local x = value -> treat as regular assignment
        NodeKind::LocalStatement | NodeKind::LocalDeclaration => {
            lower_local_statement(walker, node, lambda_ctx)
        }
        NodeKind::UsingStatement => {
            // Using statements are handled in module definitions in lowering/mod.rs
            // When encountered here, convert to a no-op statement (using is compile-time only)
            Ok(Stmt::Expr {
                expr: Expr::Literal(crate::ir::core::Literal::Nothing, span),
                span,
            })
        }
        NodeKind::ImportStatement => {
            // Import statements are handled in module definitions in lowering/mod.rs
            // When encountered here, convert to a no-op statement (import is compile-time only)
            Ok(Stmt::Expr {
                expr: Expr::Literal(crate::ir::core::Literal::Nothing, span),
                span,
            })
        }
        NodeKind::PublicStatement => {
            // Public statements are handled in module definitions in lowering/mod.rs
            // When encountered here, convert to a no-op statement (public is compile-time only)
            Ok(Stmt::Expr {
                expr: Expr::Literal(crate::ir::core::Literal::Nothing, span),
                span,
            })
        }
        NodeKind::ModuleDefinition | NodeKind::BaremoduleDefinition => {
            // Module definitions are handled at the top level in lowering/mod.rs
            Err(
                UnsupportedFeature::new(UnsupportedFeatureKind::ModuleDefinition, span)
                    .with_hint("module definitions must be at the top level"),
            )
        }
        NodeKind::ExportStatement => {
            let names = crate::lowering::lower_export_statement(walker, node)?;
            Ok(Stmt::Export { names, span })
        }
        // Function definitions inside blocks (e.g., inside @testset)
        NodeKind::FunctionDefinition => {
            let funcs = match lambda_ctx {
                Some(ctx) if contains_macro_call(walker, node) => {
                    function::lower_function_all_with_ctx(walker, node, ctx)?
                }
                _ => function::lower_function_all(walker, node)?,
            };
            Ok(lower_function_defs_to_stmt(funcs, span))
        }
        // Short function definitions (e.g., f(x) = 2x inside blocks)
        NodeKind::ShortFunctionDefinition => {
            let func = match lambda_ctx {
                Some(ctx) if contains_macro_call(walker, node) => {
                    function::lower_operator_method_with_ctx(walker, node, ctx)?
                }
                _ => function::lower_operator_method(walker, node)?,
            };
            Ok(Stmt::FunctionDef {
                func: Box::new(func),
                span,
            })
        }
        NodeKind::BinaryExpression => {
            if let Some(ctx) = lambda_ctx {
                if let Some(stmt) = try_lower_test_isa_macro_with_ctx(walker, node, ctx)? {
                    return Ok(stmt);
                }
            }
            // Check if this is an assignment (BinaryExpression with = operator)
            let is_assignment = walker
                .children(&node)
                .iter()
                .any(|child| child.kind() == "operator" && walker.text(child) == "=");
            if is_assignment {
                return lower_assignment_maybe_ctx(walker, node, lambda_ctx);
            }
            let expr = lower_expr_maybe_ctx(walker, node, lambda_ctx)?;
            Ok(Stmt::Expr { expr, span })
        }
        _ => {
            let expr = lower_expr_maybe_ctx(walker, node, lambda_ctx)?;
            Ok(Stmt::Expr { expr, span })
        }
    }
}

/// Lower a block node to Core IR.
pub fn lower_block<'a>(walker: &CstWalker<'a>, node: Node<'a>) -> LowerResult<Block> {
    lower_block_impl(walker, node, None)
}

fn lower_block_impl<'a>(
    walker: &CstWalker<'a>,
    node: Node<'a>,
    lambda_ctx: Option<&LambdaContext>,
) -> LowerResult<Block> {
    // For CompoundStatement (begin...end) or BeginBlock (mapped to Block),
    // extract the inner block if it exists
    let kind = walker.kind(&node);
    let actual_block = if kind == NodeKind::CompoundStatement || kind == NodeKind::Block {
        // Check if this node has an inner Block child (BeginBlock case)
        walker
            .named_children(&node)
            .into_iter()
            .find(|child| walker.kind(child) == NodeKind::Block)
            .unwrap_or(node)
    } else {
        node
    };

    let mut stmts = Vec::new();
    for child in walker.named_children(&actual_block) {
        // Skip comments
        match walker.kind(&child) {
            NodeKind::LineComment | NodeKind::BlockComment => continue,
            _ => {}
        }
        stmts.push(lower_stmt_impl(walker, child, lambda_ctx)?);
    }
    Ok(Block {
        stmts,
        span: walker.span(&node),
    })
}

/// Lower a statement with lambda context for collecting lifted functions.
/// This version handles arrow functions and do syntax in expressions.
pub fn lower_stmt_with_ctx<'a>(
    walker: &CstWalker<'a>,
    node: Node<'a>,
    lambda_ctx: &LambdaContext,
) -> LowerResult<Stmt> {
    lower_stmt_impl(walker, node, Some(lambda_ctx))
}

/// Lower a block with lambda context.
pub fn lower_block_with_ctx<'a>(
    walker: &CstWalker<'a>,
    node: Node<'a>,
    lambda_ctx: &LambdaContext,
) -> LowerResult<Block> {
    lower_block_impl(walker, node, Some(lambda_ctx))
}

fn try_lower_test_isa_macro_with_ctx<'a>(
    walker: &CstWalker<'a>,
    node: Node<'a>,
    lambda_ctx: &LambdaContext,
) -> LowerResult<Option<Stmt>> {
    if walker.kind(&node) != NodeKind::BinaryExpression {
        return Ok(None);
    }

    let span = walker.span(&node);
    let operands: Vec<_> = walker
        .named_children(&node)
        .into_iter()
        .filter(|child| walker.kind(child) != NodeKind::Operator)
        .collect();
    if operands.len() < 2 {
        return Ok(None);
    }

    let op_text = walker
        .children(&node)
        .into_iter()
        .find(|child| child.kind() == "operator")
        .map(|child| walker.text(&child));
    if op_text != Some("isa") {
        return Ok(None);
    }

    let left = operands[0];
    let right = operands[1];

    if walker.kind(&left) != NodeKind::MacroCall || walker.kind(&right) != NodeKind::TupleExpression
    {
        return Ok(None);
    }

    let macro_ident = walker.find_child(&left, NodeKind::MacroIdentifier);
    let macro_name = match macro_ident {
        Some(ident) => walker.text(&ident).trim_start_matches('@').to_string(),
        None => return Ok(None),
    };
    if macro_name != "test" {
        return Ok(None);
    }

    if !lambda_ctx.has_using("Test") {
        return Err(
            UnsupportedFeature::new(UnsupportedFeatureKind::MacroCall, span)
                .with_hint("@test macro requires `using Test`"),
        );
    }

    let args_nodes = walker.named_children(&right);
    if args_nodes.is_empty() {
        return Err(UnsupportedFeature::new(
            UnsupportedFeatureKind::MacroCall,
            span,
        ));
    }

    let mut args = Vec::with_capacity(args_nodes.len());
    for arg in args_nodes {
        args.push(expr::lower_expr_with_ctx(walker, arg, lambda_ctx)?);
    }

    let condition = Expr::Builtin {
        name: BuiltinOp::Isa,
        args,
        span,
    };

    Ok(Some(Stmt::Test {
        condition,
        message: None,
        span,
    }))
}

// ==================== Return Statement ====================

fn lower_return<'a>(
    walker: &CstWalker<'a>,
    node: Node<'a>,
    lambda_ctx: Option<&LambdaContext>,
) -> LowerResult<Stmt> {
    let span = walker.span(&node);
    let mut named = walker.named_children(&node);
    let value = match named.pop() {
        Some(value_node) => Some(lower_expr_maybe_ctx(walker, value_node, lambda_ctx)?),
        None => None,
    };
    Ok(Stmt::Return { value, span })
}

// ==================== Const Statement ====================

/// Lower a const statement to Core IR.
/// Type alias consts like `const ComplexF64 = Complex{Float64}` are skipped.
fn lower_const_statement<'a>(
    walker: &CstWalker<'a>,
    node: Node<'a>,
    lambda_ctx: Option<&LambdaContext>,
) -> LowerResult<Stmt> {
    let span = walker.span(&node);
    let children = walker.named_children(&node);

    for child in children {
        match walker.kind(&child) {
            NodeKind::Assignment => {
                let stmt = match lambda_ctx {
                    Some(ctx) => lower_assignment_with_ctx(walker, child, ctx),
                    None => lower_assignment(walker, child),
                }?;
                return Ok(wrap_const_assignment(stmt, span));
            }
            NodeKind::BinaryExpression => {
                // Pure Rust parser creates BinaryExpression for const assignments
                // Structure: BinaryExpression -> Identifier, Operator("="), value
                let expr_children = walker.named_children(&child);
                if expr_children.len() >= 2 {
                    let rhs = &expr_children[expr_children.len() - 1];
                    // Skip type alias assignments (const X = Type{Param})
                    if walker.kind(rhs) == NodeKind::ParametrizedTypeExpression {
                        return Ok(Stmt::Expr {
                            expr: Expr::Literal(crate::ir::core::Literal::Nothing, span),
                            span,
                        });
                    }
                    let stmt = match lambda_ctx {
                        Some(ctx) => lower_assignment_with_ctx(walker, child, ctx),
                        None => lower_assignment(walker, child),
                    }?;
                    return Ok(wrap_const_assignment(stmt, span));
                }
            }
            _ => {}
        }
    }
    // If no assignment found, return a no-op
    Ok(Stmt::Expr {
        expr: Expr::Literal(crate::ir::core::Literal::Nothing, span),
        span,
    })
}

fn wrap_const_assignment(stmt: Stmt, span: Span) -> Stmt {
    if let Stmt::Assign { var, .. } = &stmt {
        Stmt::Block(Block {
            stmts: vec![
                Stmt::Expr {
                    expr: Expr::Call {
                        function: "#__sjulia_declare_const__".to_string(),
                        args: vec![Expr::Literal(
                            crate::ir::core::Literal::Str(var.clone()),
                            span,
                        )],
                        kwargs: Vec::new(),
                        splat_mask: vec![false],
                        kwargs_splat_mask: Vec::new(),
                        span,
                    },
                    span,
                },
                stmt,
            ],
            span,
        })
    } else {
        stmt
    }
}

// ==================== Global Statement ====================

/// Lower a global statement to Core IR.
///
/// A bare `global x` (or `global x, y`) lowers to a `Stmt::Global` marker that
/// records the declared names. `global x = value` and `global x += value` lower
/// to that same marker followed by the assignment, so the compiler can route
/// the binding to the module-level frame (Issues #5548, #5549).
fn lower_global_statement<'a>(
    walker: &CstWalker<'a>,
    node: Node<'a>,
    lambda_ctx: Option<&LambdaContext>,
) -> LowerResult<Stmt> {
    let span = walker.span(&node);
    let children = walker.named_children(&node);

    // Look for assignments or compound assignments in the children
    for child in &children {
        let assign_stmt = match walker.kind(child) {
            NodeKind::Assignment => Some(lower_assignment_maybe_ctx(walker, *child, lambda_ctx)?),
            NodeKind::BinaryExpression => {
                // Check if this is an assignment (BinaryExpression with = operator)
                let is_assignment = walker
                    .children(child)
                    .iter()
                    .any(|c| c.kind() == "operator" && walker.text(c) == "=");
                if is_assignment {
                    Some(lower_assignment_maybe_ctx(walker, *child, lambda_ctx)?)
                } else {
                    None
                }
            }
            NodeKind::CompoundAssignment => Some(lower_compound_assignment_maybe_ctx(
                walker, *child, lambda_ctx,
            )?),
            _ => None,
        };

        if let Some(stmt) = assign_stmt {
            // Prefix the assignment with a `global` marker so the compiler routes
            // both the read and the write to the module-level binding.
            return Ok(match assigned_var_names(&stmt) {
                Some(names) if !names.is_empty() => Stmt::Block(Block {
                    stmts: vec![Stmt::Global { names, span }, stmt],
                    span,
                }),
                _ => stmt,
            });
        }
    }

    // No assignment found - a bare declaration like `global x` or `global x, y`.
    let names = collect_global_decl_names(walker, &children);
    Ok(Stmt::Global { names, span })
}

/// Extract the variable name(s) bound by a lowered assignment statement so a
/// `global` declaration can be attached to them. Returns `None` for assignment
/// forms that mutate an existing object (index/field/dict assignment) rather
/// than rebinding a name.
fn assigned_var_names(stmt: &Stmt) -> Option<Vec<String>> {
    match stmt {
        Stmt::Assign { var, .. } | Stmt::AddAssign { var, .. } => Some(vec![var.clone()]),
        Stmt::DestructuringAssign { targets, .. } => Some(targets.clone()),
        _ => None,
    }
}

/// Collect the identifier names from a bare `global` declaration such as
/// `global x` or `global x, y`.
fn collect_global_decl_names<'a>(walker: &CstWalker<'a>, children: &[Node<'a>]) -> Vec<String> {
    let mut names = Vec::new();
    for child in children {
        match walker.kind(child) {
            NodeKind::Identifier => names.push(walker.text(child).to_string()),
            NodeKind::TypedExpression => {
                // `global x::T` — record the leading identifier.
                if let Some(id) = walker
                    .named_children(child)
                    .into_iter()
                    .find(|c| walker.kind(c) == NodeKind::Identifier)
                {
                    names.push(walker.text(&id).to_string());
                }
            }
            NodeKind::TupleExpression => {
                // `global (x, y)` — record each identifier element.
                for inner in walker.named_children(child) {
                    if walker.kind(&inner) == NodeKind::Identifier {
                        names.push(walker.text(&inner).to_string());
                    }
                }
            }
            _ => {}
        }
    }
    names
}

// ==================== Local Statement ====================

/// Lower a local statement to Core IR.
/// `local x = value` is treated as regular assignment.
fn lower_local_statement<'a>(
    walker: &CstWalker<'a>,
    node: Node<'a>,
    lambda_ctx: Option<&LambdaContext>,
) -> LowerResult<Stmt> {
    let span = walker.span(&node);
    let children = walker.named_children(&node);
    for child in children {
        match walker.kind(&child) {
            // `local f(args) = body` / `local f(args) where {T} = body`: the item
            // is a short-form function definition, not a variable assignment
            // (Issue #8065). Lower it as a function definition like a top-level
            // short form, otherwise the `Call`/`Where` LHS is rejected as an
            // unsupported assignment target.
            NodeKind::Assignment if function::is_short_function_definition(walker, child) => {
                let funcs = match lambda_ctx {
                    Some(ctx) if contains_macro_call(walker, child) => {
                        function::lower_short_function_all_with_ctx(walker, child, ctx)?
                    }
                    _ => function::lower_short_function_all(walker, child)?,
                };
                return Ok(lower_function_defs_to_stmt(funcs, span));
            }
            NodeKind::Assignment => return lower_assignment_maybe_ctx(walker, child, lambda_ctx),
            NodeKind::BinaryExpression if walker.named_children(&child).len() >= 2 => {
                return lower_assignment_maybe_ctx(walker, child, lambda_ctx);
            }
            NodeKind::Identifier => continue,
            _ => {}
        }
    }
    Ok(Stmt::Expr {
        expr: Expr::Literal(crate::ir::core::Literal::Nothing, span),
        span,
    })
}

fn lower_expr_maybe_ctx<'a>(
    walker: &CstWalker<'a>,
    node: Node<'a>,
    lambda_ctx: Option<&LambdaContext>,
) -> LowerResult<Expr> {
    match lambda_ctx {
        Some(ctx) => expr::lower_expr_with_ctx(walker, node, ctx),
        None => expr::lower_expr(walker, node),
    }
}

fn lower_assignment_maybe_ctx<'a>(
    walker: &CstWalker<'a>,
    node: Node<'a>,
    lambda_ctx: Option<&LambdaContext>,
) -> LowerResult<Stmt> {
    match lambda_ctx {
        Some(ctx) => lower_assignment_with_ctx(walker, node, ctx),
        None => lower_assignment(walker, node),
    }
}

fn lower_compound_assignment_maybe_ctx<'a>(
    walker: &CstWalker<'a>,
    node: Node<'a>,
    lambda_ctx: Option<&LambdaContext>,
) -> LowerResult<Stmt> {
    match lambda_ctx {
        Some(ctx) => lower_compound_assignment_with_ctx(walker, node, ctx),
        None => lower_compound_assignment(walker, node),
    }
}

fn lower_for_stmt_maybe_ctx<'a>(
    walker: &CstWalker<'a>,
    node: Node<'a>,
    lambda_ctx: Option<&LambdaContext>,
) -> LowerResult<Stmt> {
    match lambda_ctx {
        Some(ctx) => lower_for_stmt_with_ctx(walker, node, ctx),
        None => lower_for_stmt(walker, node),
    }
}

fn lower_if_stmt_maybe_ctx<'a>(
    walker: &CstWalker<'a>,
    node: Node<'a>,
    lambda_ctx: Option<&LambdaContext>,
) -> LowerResult<Stmt> {
    match lambda_ctx {
        Some(ctx) => lower_if_stmt_with_ctx(walker, node, ctx),
        None => lower_if_stmt(walker, node),
    }
}

fn lower_while_stmt_maybe_ctx<'a>(
    walker: &CstWalker<'a>,
    node: Node<'a>,
    lambda_ctx: Option<&LambdaContext>,
) -> LowerResult<Stmt> {
    match lambda_ctx {
        Some(ctx) => lower_while_stmt_with_ctx(walker, node, ctx),
        None => lower_while_stmt(walker, node),
    }
}

fn lower_try_stmt_maybe_ctx<'a>(
    walker: &CstWalker<'a>,
    node: Node<'a>,
    lambda_ctx: Option<&LambdaContext>,
) -> LowerResult<Stmt> {
    match lambda_ctx {
        Some(ctx) => lower_try_stmt_with_ctx(walker, node, ctx),
        None => lower_try_stmt(walker, node),
    }
}

fn lower_macro_maybe_ctx<'a>(
    walker: &CstWalker<'a>,
    node: Node<'a>,
    lambda_ctx: Option<&LambdaContext>,
) -> LowerResult<Stmt> {
    match lambda_ctx {
        Some(ctx) => lower_macro_with_ctx(walker, node, ctx),
        None => lower_macro(walker, node),
    }
}

// ==================== Type Alias Detection ====================

use crate::ir::core::TypeAliasDef;

/// Check if a const statement is a type alias definition.
/// Type aliases have the form: `const Name = TypeExpr` or the parametric form
/// `const Name{T...} = TypeExpr{T...}` where TypeExpr is a type expression.
/// Returns Some(TypeAliasDef) if this is a type alias, None otherwise.
pub fn try_extract_type_alias<'a>(walker: &CstWalker<'a>, node: Node<'a>) -> Option<TypeAliasDef> {
    if walker.kind(&node) != NodeKind::ConstStatement {
        return None;
    }

    let span = walker.span(&node);
    let children = walker.named_children(&node);

    for child in children {
        let child_kind = walker.kind(&child);
        if child_kind == NodeKind::BinaryExpression || child_kind == NodeKind::Assignment {
            if let Some(alias) = extract_type_alias_from_binding(walker, child, span) {
                return Some(alias);
            }
        }
    }

    None
}

/// Extract a type alias from a plain (non-`const`) assignment node such as
/// `MyVec{T} = Vector{T}` or `IntVec = Vector{Int}` (Issue #5055). Returns
/// `None` when the node is not a structured type alias (e.g. an ordinary value
/// assignment or a short function definition).
pub fn try_extract_type_alias_from_assignment<'a>(
    walker: &CstWalker<'a>,
    node: Node<'a>,
) -> Option<TypeAliasDef> {
    if walker.kind(&node) != NodeKind::Assignment {
        return None;
    }
    let span = walker.span(&node);
    extract_type_alias_from_binding(walker, node, span)
}

/// Shared `LHS = RHS` type-alias extraction for both `const` and plain
/// assignment forms. Handles a bare identifier LHS (`Name = T`) and a
/// parametric LHS (`Name{P...} = T{P...}`).
fn extract_type_alias_from_binding<'a>(
    walker: &CstWalker<'a>,
    binding: Node<'a>,
    span: Span,
) -> Option<TypeAliasDef> {
    let expr_children = walker.named_children(&binding);
    if expr_children.len() < 2 {
        return None;
    }
    let lhs = &expr_children[0];
    let rhs = &expr_children[expr_children.len() - 1];
    let rhs_kind = walker.kind(rhs);

    // `Name = Body where {T...}` is a runtime UnionAll-valued binding, not a
    // static string alias. Registering it as a non-parametric alias makes
    // `Name{Args...}` expand to the unsubstituted target and ignore `Args`
    // (Issue #5053). Parametric alias syntax (`Name{T} = Target{T}`) remains
    // handled below.
    if rhs_kind == NodeKind::WhereExpression {
        return None;
    }

    // RHS must be a type expression for this to be a type alias.
    if !is_type_expression(walker, rhs, rhs_kind) {
        return None;
    }

    let target_type = walker.text(rhs).to_string();

    match walker.kind(lhs) {
        // Non-parametric alias: `Name = TypeExpr`.
        NodeKind::Identifier => Some(TypeAliasDef {
            name: walker.text(lhs).to_string(),
            target_type,
            params: Vec::new(),
            span,
        }),
        // Parametric alias: `Name{P...} = TypeExpr` (Issue #5055).
        NodeKind::ParametrizedTypeExpression => {
            let (name, params) = extract_parametric_alias_lhs(walker, *lhs)?;
            Some(TypeAliasDef {
                name,
                target_type,
                params,
                span,
            })
        }
        _ => None,
    }
}

/// Extract `(base_name, params)` from a parametric type-alias LHS such as
/// `MyVec{T}` -> `("MyVec", ["T"])` or `MyDict{K, V}` -> `("MyDict", ["K","V"])`.
/// Returns `None` if no parameters are present (a bare parametric head is not a
/// valid parametric alias LHS).
fn extract_parametric_alias_lhs<'a>(
    walker: &CstWalker<'a>,
    lhs: Node<'a>,
) -> Option<(String, Vec<String>)> {
    let children = walker.named_children(&lhs);
    let mut base: Option<String> = None;
    let mut params: Vec<String> = Vec::new();
    for child in children {
        match walker.kind(&child) {
            NodeKind::Identifier => {
                if base.is_none() {
                    base = Some(walker.text(&child).to_string());
                } else {
                    params.push(walker.text(&child).to_string());
                }
            }
            NodeKind::CurlyExpression => {
                for p in walker.named_children(&child) {
                    // A parameter may carry a bound (`T<:Number`); record just
                    // the leading identifier name for positional substitution.
                    if let Some(pname) = first_identifier_text(walker, p) {
                        params.push(pname);
                    }
                }
            }
            NodeKind::TypeParameterList => {
                for p in walker.named_children(&child) {
                    if let Some(pname) = first_identifier_text(walker, p) {
                        params.push(pname);
                    }
                }
            }
            _ => {
                if base.is_some() {
                    if let Some(pname) = first_identifier_text(walker, child) {
                        params.push(pname);
                    }
                }
            }
        }
    }
    let base = base?;
    if params.is_empty() {
        return None;
    }
    Some((base, params))
}

/// Return the text of the first `Identifier` at or under `node` (the node
/// itself if it is an identifier), used to read a type-parameter name.
fn first_identifier_text<'a>(walker: &CstWalker<'a>, node: Node<'a>) -> Option<String> {
    if walker.kind(&node) == NodeKind::Identifier {
        return Some(walker.text(&node).to_string());
    }
    for child in walker.named_children(&node) {
        if walker.kind(&child) == NodeKind::Identifier {
            return Some(walker.text(&child).to_string());
        }
    }
    None
}

/// Check if a node represents a type expression.
fn is_type_expression<'a>(walker: &CstWalker<'a>, node: &Node<'a>, kind: NodeKind) -> bool {
    match kind {
        // Parametrized types: Complex{Float64}, Array{Int64, 2}
        NodeKind::ParametrizedTypeExpression => true,
        // Union types: Union{Int64, Float64}
        NodeKind::CurlyExpression => {
            let text = walker.text(node);
            text.starts_with("Union{")
        }
        // Type identifiers that look like type names (start with uppercase)
        NodeKind::Identifier => {
            let text = walker.text(node);
            // Check if it looks like a type name (uppercase first letter)
            // and is a known type or ends with a common type suffix
            is_likely_type_name(text)
        }
        _ => false,
    }
}

/// Check if an identifier looks like a type name.
fn is_likely_type_name(name: &str) -> bool {
    if name.is_empty() {
        return false;
    }

    let first_char = name.chars().next().unwrap();
    if !first_char.is_ascii_uppercase() {
        return false;
    }

    // Check against known builtin types
    matches!(
        name,
        "Int8"
            | "Int16"
            | "Int32"
            | "Int64"
            | "Int128"
            | "Int"
            | "UInt8"
            | "UInt16"
            | "UInt32"
            | "UInt64"
            | "UInt128"
            | "UInt"
            | "Float16"
            | "Float32"
            | "Float64"
            | "Bool"
            | "String"
            | "Char"
            | "Array"
            | "Vector"
            | "Matrix"
            | "DenseArray"
            | "DenseVector"
            | "DenseMatrix"
            | "Tuple"
            | "NamedTuple"
            | "Dict"
            | "Set"
            | "Any"
            | "Number"
            | "Real"
            | "Integer"
            | "AbstractFloat"
            | "Nothing"
            | "Missing"
            | "Complex"
            | "Rational"
    )
}
