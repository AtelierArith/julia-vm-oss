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
mod macros;

use crate::error::{UnsupportedFeature, UnsupportedFeatureKind};
use crate::ir::core::{Block, BuiltinOp, Expr, Function, Stmt};
use crate::lowering::expr;
use crate::lowering::function;
use crate::lowering::{LambdaContext, LowerResult};
use crate::parser::cst::{CstWalker, Node, NodeKind};

// Re-export public functions
pub use assignment::{
    lower_assignment, lower_assignment_with_ctx, lower_compound_assignment,
    lower_compound_assignment_with_ctx,
};
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
    let span = walker.span(&node);
    match walker.kind(&node) {
        // Assignment can also be a short function definition (f(x) = 2x) - check this first
        NodeKind::Assignment if function::is_short_function_definition(walker, node) => {
            let funcs = function::lower_short_function_all(walker, node)?;
            Ok(lower_function_defs_to_stmt(funcs, span))
        }
        NodeKind::Assignment => lower_assignment(walker, node),
        NodeKind::CompoundAssignment => lower_compound_assignment(walker, node),
        NodeKind::ReturnStatement => lower_return(walker, node),
        NodeKind::ForStatement => lower_for_stmt(walker, node),
        NodeKind::IfStatement => lower_if_stmt(walker, node),
        NodeKind::WhileStatement => lower_while_stmt(walker, node),
        NodeKind::BreakStatement => Ok(Stmt::Break { span }),
        NodeKind::ContinueStatement => Ok(Stmt::Continue { span }),
        NodeKind::TryStatement => lower_try_stmt(walker, node),
        NodeKind::MacroCall => lower_macro(walker, node),
        // const x = 1 -> treat as regular assignment (simplified implementation)
        NodeKind::ConstStatement => lower_const_statement(walker, node),
        // global x -> ignored (simplified implementation - no global scope tracking)
        NodeKind::GlobalStatement => lower_global_statement(walker, node),
        // local x = value -> treat as regular assignment
        NodeKind::LocalStatement | NodeKind::LocalDeclaration => {
            lower_local_statement(walker, node)
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
            // Export statements are handled in module definitions in lowering/mod.rs
            // When encountered here, convert to a no-op statement (export is compile-time only)
            Ok(Stmt::Expr {
                expr: Expr::Literal(crate::ir::core::Literal::Nothing, span),
                span,
            })
        }
        // Function definitions inside blocks (e.g., inside @testset)
        NodeKind::FunctionDefinition => {
            let funcs = function::lower_function_all(walker, node)?;
            Ok(lower_function_defs_to_stmt(funcs, span))
        }
        // Short function definitions (e.g., f(x) = 2x inside blocks)
        NodeKind::ShortFunctionDefinition => {
            let func = function::lower_operator_method(walker, node)?;
            Ok(Stmt::FunctionDef {
                func: Box::new(func),
                span,
            })
        }
        NodeKind::BinaryExpression => {
            // Check if this is an assignment (BinaryExpression with = operator)
            let is_assignment = walker
                .children(&node)
                .iter()
                .any(|child| child.kind() == "operator" && walker.text(child) == "=");
            if is_assignment {
                return lower_assignment(walker, node);
            }
            let expr = expr::lower_expr(walker, node)?;
            Ok(Stmt::Expr { expr, span })
        }
        _ => {
            let expr = expr::lower_expr(walker, node)?;
            Ok(Stmt::Expr { expr, span })
        }
    }
}

/// Lower a block node to Core IR.
pub fn lower_block<'a>(walker: &CstWalker<'a>, node: Node<'a>) -> LowerResult<Block> {
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
        stmts.push(lower_stmt(walker, child)?);
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
    let span = walker.span(&node);
    match walker.kind(&node) {
        // Assignment can also be a short function definition (f(x) = 2x) - check this first
        NodeKind::Assignment if function::is_short_function_definition(walker, node) => {
            let funcs = function::lower_short_function_all(walker, node)?;
            Ok(lower_function_defs_to_stmt(funcs, span))
        }
        NodeKind::Assignment => lower_assignment_with_ctx(walker, node, lambda_ctx),
        NodeKind::CompoundAssignment => {
            lower_compound_assignment_with_ctx(walker, node, lambda_ctx)
        }
        NodeKind::ReturnStatement => lower_return_with_ctx(walker, node, lambda_ctx),
        NodeKind::ForStatement => lower_for_stmt_with_ctx(walker, node, lambda_ctx),
        NodeKind::IfStatement => lower_if_stmt_with_ctx(walker, node, lambda_ctx),
        NodeKind::WhileStatement => lower_while_stmt_with_ctx(walker, node, lambda_ctx),
        NodeKind::BreakStatement => Ok(Stmt::Break { span }),
        NodeKind::ContinueStatement => Ok(Stmt::Continue { span }),
        NodeKind::TryStatement => lower_try_stmt_with_ctx(walker, node, lambda_ctx),
        NodeKind::MacroCall => lower_macro_with_ctx(walker, node, lambda_ctx),
        // const x = 1 -> treat as regular assignment (simplified implementation)
        NodeKind::ConstStatement => lower_const_statement_with_ctx(walker, node, lambda_ctx),
        // global x = value or global x += value -> treat as assignment
        NodeKind::GlobalStatement => lower_global_statement_with_ctx(walker, node, lambda_ctx),
        // local x = value -> treat as regular assignment
        NodeKind::LocalStatement | NodeKind::LocalDeclaration => {
            lower_local_statement_with_ctx(walker, node, lambda_ctx)
        }
        NodeKind::BinaryExpression => {
            if let Some(stmt) = try_lower_test_isa_macro_with_ctx(walker, node, lambda_ctx)? {
                return Ok(stmt);
            }
            // Check if this is an assignment (BinaryExpression with = operator)
            let is_assignment = walker
                .children(&node)
                .iter()
                .any(|child| child.kind() == "operator" && walker.text(child) == "=");
            if is_assignment {
                return lower_assignment_with_ctx(walker, node, lambda_ctx);
            }
            let expr = expr::lower_expr_with_ctx(walker, node, lambda_ctx)?;
            Ok(Stmt::Expr { expr, span })
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
        NodeKind::ModuleDefinition => Err(UnsupportedFeature::new(
            UnsupportedFeatureKind::ModuleDefinition,
            span,
        )
        .with_hint("module definitions must be at the top level")),
        NodeKind::ExportStatement => {
            // Export statements are handled in module definitions in lowering/mod.rs
            // When encountered here, convert to a no-op statement (export is compile-time only)
            Ok(Stmt::Expr {
                expr: Expr::Literal(crate::ir::core::Literal::Nothing, span),
                span,
            })
        }
        // Function definitions inside blocks (e.g., inside @testset)
        NodeKind::FunctionDefinition => {
            let funcs = function::lower_function_all(walker, node)?;
            Ok(lower_function_defs_to_stmt(funcs, span))
        }
        // Short function definitions (e.g., f(x) = 2x inside blocks)
        NodeKind::ShortFunctionDefinition => {
            let func = function::lower_operator_method(walker, node)?;
            Ok(Stmt::FunctionDef {
                func: Box::new(func),
                span,
            })
        }
        _ => {
            let expr = expr::lower_expr_with_ctx(walker, node, lambda_ctx)?;
            Ok(Stmt::Expr { expr, span })
        }
    }
}

/// Lower a block with lambda context.
pub fn lower_block_with_ctx<'a>(
    walker: &CstWalker<'a>,
    node: Node<'a>,
    lambda_ctx: &LambdaContext,
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
        stmts.push(lower_stmt_with_ctx(walker, child, lambda_ctx)?);
    }
    Ok(Block {
        stmts,
        span: walker.span(&node),
    })
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

fn lower_return<'a>(walker: &CstWalker<'a>, node: Node<'a>) -> LowerResult<Stmt> {
    let span = walker.span(&node);
    let mut named = walker.named_children(&node);
    let value = match named.pop() {
        Some(value_node) => Some(expr::lower_expr(walker, value_node)?),
        None => None,
    };
    Ok(Stmt::Return { value, span })
}

fn lower_return_with_ctx<'a>(
    walker: &CstWalker<'a>,
    node: Node<'a>,
    lambda_ctx: &LambdaContext,
) -> LowerResult<Stmt> {
    let span = walker.span(&node);
    let mut named = walker.named_children(&node);
    let value = match named.pop() {
        Some(value_node) => Some(expr::lower_expr_with_ctx(walker, value_node, lambda_ctx)?),
        None => None,
    };
    Ok(Stmt::Return { value, span })
}

// ==================== Const Statement ====================

/// Lower a const statement to Core IR.
/// `const x = 1` is treated as a regular assignment (simplified implementation).
/// Type alias consts like `const ComplexF64 = Complex{Float64}` are skipped.
fn lower_const_statement<'a>(walker: &CstWalker<'a>, node: Node<'a>) -> LowerResult<Stmt> {
    let span = walker.span(&node);
    let children = walker.named_children(&node);

    for child in children {
        let child_kind = walker.kind(&child);
        match child_kind {
            NodeKind::Assignment => {
                return lower_assignment(walker, child);
            }
            NodeKind::BinaryExpression => {
                // Pure Rust parser creates BinaryExpression for const assignments
                // Structure: BinaryExpression -> Identifier, Operator("="), value
                let expr_children = walker.named_children(&child);
                if expr_children.len() >= 2 {
                    let rhs = &expr_children[expr_children.len() - 1];
                    let rhs_kind = walker.kind(rhs);
                    // Skip type alias assignments (const X = Type{Param})
                    if rhs_kind == NodeKind::ParametrizedTypeExpression {
                        return Ok(Stmt::Expr {
                            expr: Expr::Literal(crate::ir::core::Literal::Nothing, span),
                            span,
                        });
                    }
                    // Handle as regular assignment
                    return lower_assignment(walker, child);
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

fn lower_const_statement_with_ctx<'a>(
    walker: &CstWalker<'a>,
    node: Node<'a>,
    lambda_ctx: &LambdaContext,
) -> LowerResult<Stmt> {
    let span = walker.span(&node);
    let children = walker.named_children(&node);

    for child in children {
        match walker.kind(&child) {
            NodeKind::Assignment => {
                return lower_assignment_with_ctx(walker, child, lambda_ctx);
            }
            NodeKind::BinaryExpression => {
                // Pure Rust parser creates BinaryExpression for const assignments
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
                    // Handle as regular assignment
                    return lower_assignment_with_ctx(walker, child, lambda_ctx);
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

// ==================== Global Statement ====================

/// Lower a global statement to Core IR.
/// `global x` is treated as a no-op, but `global x = value` and `global x += value`
/// are treated as assignments.
fn lower_global_statement<'a>(walker: &CstWalker<'a>, node: Node<'a>) -> LowerResult<Stmt> {
    let span = walker.span(&node);
    let children = walker.named_children(&node);

    // Look for assignments or compound assignments in the children
    for child in children {
        match walker.kind(&child) {
            NodeKind::Assignment => return lower_assignment(walker, child),
            NodeKind::BinaryExpression => {
                // Check if this is an assignment (BinaryExpression with = operator)
                let is_assignment = walker
                    .children(&child)
                    .iter()
                    .any(|c| c.kind() == "operator" && walker.text(c) == "=");
                if is_assignment {
                    return lower_assignment(walker, child);
                }
            }
            NodeKind::CompoundAssignment => {
                return lower_compound_assignment(walker, child);
            }
            NodeKind::Identifier => continue,
            NodeKind::TypedExpression => continue,
            _ => {}
        }
    }

    // No assignment found - just a declaration like `global x`
    Ok(Stmt::Expr {
        expr: Expr::Literal(crate::ir::core::Literal::Nothing, span),
        span,
    })
}

/// Lower a global statement with lambda context.
fn lower_global_statement_with_ctx<'a>(
    walker: &CstWalker<'a>,
    node: Node<'a>,
    lambda_ctx: &LambdaContext,
) -> LowerResult<Stmt> {
    let span = walker.span(&node);
    let children = walker.named_children(&node);

    // Look for assignments or compound assignments in the children
    for child in children {
        match walker.kind(&child) {
            NodeKind::Assignment => return lower_assignment_with_ctx(walker, child, lambda_ctx),
            NodeKind::BinaryExpression => {
                // Check if this is an assignment (BinaryExpression with = operator)
                let is_assignment = walker
                    .children(&child)
                    .iter()
                    .any(|c| c.kind() == "operator" && walker.text(c) == "=");
                if is_assignment {
                    return lower_assignment_with_ctx(walker, child, lambda_ctx);
                }
            }
            NodeKind::CompoundAssignment => {
                return lower_compound_assignment_with_ctx(walker, child, lambda_ctx);
            }
            NodeKind::Identifier => continue,
            NodeKind::TypedExpression => continue,
            _ => {}
        }
    }

    // No assignment found - just a declaration like `global x`
    Ok(Stmt::Expr {
        expr: Expr::Literal(crate::ir::core::Literal::Nothing, span),
        span,
    })
}

// ==================== Local Statement ====================

/// Lower a local statement to Core IR.
/// `local x = value` is treated as regular assignment.
fn lower_local_statement<'a>(walker: &CstWalker<'a>, node: Node<'a>) -> LowerResult<Stmt> {
    let span = walker.span(&node);
    let children = walker.named_children(&node);
    for child in children {
        match walker.kind(&child) {
            NodeKind::Assignment => return lower_assignment(walker, child),
            NodeKind::BinaryExpression => {
                if walker.named_children(&child).len() >= 2 {
                    return lower_assignment(walker, child);
                }
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

/// Lower a local statement with lambda context.
fn lower_local_statement_with_ctx<'a>(
    walker: &CstWalker<'a>,
    node: Node<'a>,
    lambda_ctx: &LambdaContext,
) -> LowerResult<Stmt> {
    let span = walker.span(&node);
    let children = walker.named_children(&node);
    for child in children {
        match walker.kind(&child) {
            NodeKind::Assignment => return lower_assignment_with_ctx(walker, child, lambda_ctx),
            NodeKind::BinaryExpression => {
                if walker.named_children(&child).len() >= 2 {
                    return lower_assignment_with_ctx(walker, child, lambda_ctx);
                }
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

// ==================== Type Alias Detection ====================

use crate::ir::core::TypeAliasDef;

/// Check if a const statement is a type alias definition.
/// Type aliases have the form: `const Name = TypeExpr` where TypeExpr is a type expression.
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
            let expr_children = walker.named_children(&child);
            if expr_children.len() >= 2 {
                let lhs = &expr_children[0];
                let rhs = &expr_children[expr_children.len() - 1];
                let rhs_kind = walker.kind(rhs);

                // Check if RHS is a type expression
                if is_type_expression(walker, rhs, rhs_kind) {
                    // Get LHS name
                    let name = match walker.kind(lhs) {
                        NodeKind::Identifier => walker.text(lhs).to_string(),
                        _ => continue,
                    };

                    // Get RHS type expression as string
                    let target_type = walker.text(rhs).to_string();

                    return Some(TypeAliasDef {
                        name,
                        target_type,
                        span,
                    });
                }
            }
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
