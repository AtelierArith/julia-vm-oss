//! Usage-based parameter type inference.
//!
//! This module analyzes how parameters are used within a function body to infer
//! their types when no type annotations are provided. This enables more precise
//! type inference for untyped parameters.
//!
//! # Analysis Strategy
//!
//! The analyzer walks the function body and collects type constraints based on:
//! - Binary operations: `x + 1` implies `x` is numeric
//! - Comparison operations: `x < y` implies both are comparable
//! - Function calls: argument types are constrained by transfer function signatures
//! - Array indexing: `arr[i]` implies `arr` is indexable, `i` is an integer
//!
//! # Example
//!
//! ```text
//! function f(x)
//!     return x + 1
//! end
//! ```
//!
//! The analyzer infers that `x` must be numeric (supports `+` with Int64).

use crate::compile::lattice::types::{ConcreteType, LatticeType};
use crate::compile::tfuncs::TransferFunctions;
use crate::ir::core::{BinaryOp, Block, Expr, Function, Stmt};
use std::collections::HashMap;

/// Infers parameter type constraints from usage patterns in a function body.
///
/// Analyzes how untyped parameters are used within the function and returns
/// a map from parameter names to their inferred types.
///
/// # Arguments
/// - `func`: The function to analyze
/// - `tfuncs`: Transfer functions for inferring call return types
///
/// # Returns
/// A map from parameter name to inferred type constraint
pub fn infer_parameter_constraints(
    func: &Function,
    _tfuncs: &TransferFunctions,
) -> HashMap<String, LatticeType> {
    let mut constraints: HashMap<String, Vec<LatticeType>> = HashMap::new();

    // Initialize constraints for all untyped parameters
    for param in &func.params {
        if param.type_annotation.is_none() {
            constraints.insert(param.name.clone(), Vec::new());
        }
    }

    // Collect constraints from the function body
    collect_block_constraints(&func.body, &mut constraints);

    // Merge constraints for each parameter using meet (intersection)
    constraints
        .into_iter()
        .map(|(name, types)| {
            if types.is_empty() {
                // No constraints found - return Top
                (name, LatticeType::Top)
            } else {
                // Meet all constraints together
                let inferred = types
                    .into_iter()
                    .fold(LatticeType::Top, |acc, ty| acc.meet(&ty));
                (name, inferred)
            }
        })
        .collect()
}

/// Collects type constraints from a block of statements.
fn collect_block_constraints(block: &Block, constraints: &mut HashMap<String, Vec<LatticeType>>) {
    for stmt in &block.stmts {
        collect_stmt_constraints(stmt, constraints);
    }
}

/// Collects type constraints from a statement.
fn collect_stmt_constraints(stmt: &Stmt, constraints: &mut HashMap<String, Vec<LatticeType>>) {
    match stmt {
        Stmt::Assign { value, .. } => {
            collect_expr_constraints(value, constraints);
        }
        Stmt::Return {
            value: Some(expr), ..
        } => {
            collect_expr_constraints(expr, constraints);
        }
        Stmt::Return { value: None, .. } => {}
        Stmt::If {
            condition,
            then_branch,
            else_branch,
            ..
        } => {
            collect_expr_constraints(condition, constraints);
            collect_block_constraints(then_branch, constraints);
            if let Some(else_blk) = else_branch {
                collect_block_constraints(else_blk, constraints);
            }
        }
        Stmt::For { body, .. } => {
            collect_block_constraints(body, constraints);
        }
        Stmt::ForEach { iterable, body, .. } => {
            collect_expr_constraints(iterable, constraints);
            collect_block_constraints(body, constraints);
        }
        Stmt::While {
            condition, body, ..
        } => {
            collect_expr_constraints(condition, constraints);
            collect_block_constraints(body, constraints);
        }
        Stmt::Expr { expr, .. } => {
            collect_expr_constraints(expr, constraints);
        }
        _ => {}
    }
}

/// Collects type constraints from an expression.
///
/// This is the core of the usage analysis - it examines how variables are
/// used in expressions to infer their types.
fn collect_expr_constraints(expr: &Expr, constraints: &mut HashMap<String, Vec<LatticeType>>) {
    match expr {
        Expr::BinaryOp {
            op, left, right, ..
        } => {
            // Recursively process operands
            collect_expr_constraints(left, constraints);
            collect_expr_constraints(right, constraints);

            // Infer type constraints based on the operation
            match op {
                BinaryOp::Add | BinaryOp::Sub | BinaryOp::Mul | BinaryOp::Div => {
                    // Arithmetic operations imply numeric types
                    add_numeric_constraint_for_expr(left, constraints);
                    add_numeric_constraint_for_expr(right, constraints);
                }
                BinaryOp::Lt | BinaryOp::Le | BinaryOp::Gt | BinaryOp::Ge => {
                    // Comparison operations also imply numeric or comparable types
                    add_numeric_constraint_for_expr(left, constraints);
                    add_numeric_constraint_for_expr(right, constraints);
                }
                BinaryOp::Eq | BinaryOp::Ne => {
                    // Equality can work with any type - no constraint added
                }
                BinaryOp::And | BinaryOp::Or => {
                    // Boolean operations imply boolean type
                    add_bool_constraint_for_expr(left, constraints);
                    add_bool_constraint_for_expr(right, constraints);
                }
                _ => {}
            }
        }
        Expr::UnaryOp { operand, .. } => {
            collect_expr_constraints(operand, constraints);
        }
        Expr::Call { args, .. } => {
            for arg in args {
                collect_expr_constraints(arg, constraints);
            }
        }
        Expr::Index { array, indices, .. } => {
            collect_expr_constraints(array, constraints);
            for idx in indices {
                collect_expr_constraints(idx, constraints);
                // Array indexing implies the index is an integer
                add_integer_constraint_for_expr(idx, constraints);
            }
        }
        Expr::ArrayLiteral { elements, .. } => {
            for elem in elements {
                collect_expr_constraints(elem, constraints);
            }
        }
        Expr::TupleLiteral { elements, .. } => {
            for elem in elements {
                collect_expr_constraints(elem, constraints);
            }
        }
        Expr::FieldAccess { object, .. } => {
            collect_expr_constraints(object, constraints);
        }
        Expr::Range {
            start, step, stop, ..
        } => {
            collect_expr_constraints(start, constraints);
            collect_expr_constraints(stop, constraints);
            if let Some(step_expr) = step {
                collect_expr_constraints(step_expr, constraints);
            }
            // Range bounds are typically numeric
            add_numeric_constraint_for_expr(start, constraints);
            add_numeric_constraint_for_expr(stop, constraints);
        }
        _ => {}
    }
}

/// Adds a numeric type constraint for a variable if the expression is a variable reference.
fn add_numeric_constraint_for_expr(
    expr: &Expr,
    constraints: &mut HashMap<String, Vec<LatticeType>>,
) {
    if let Expr::Var(name, _) = expr {
        if let Some(constraint_list) = constraints.get_mut(name) {
            // Use the abstract Number type for numeric constraints
            // This correctly represents that x could be any numeric type (Int*, UInt*, Float*, etc.)
            constraint_list.push(LatticeType::Concrete(ConcreteType::Number));
        }
    }
}

/// Adds an integer type constraint for a variable if the expression is a variable reference.
fn add_integer_constraint_for_expr(
    expr: &Expr,
    constraints: &mut HashMap<String, Vec<LatticeType>>,
) {
    if let Expr::Var(name, _) = expr {
        if let Some(constraint_list) = constraints.get_mut(name) {
            constraint_list.push(LatticeType::Concrete(ConcreteType::Int64));
        }
    }
}

/// Adds a boolean type constraint for a variable if the expression is a variable reference.
fn add_bool_constraint_for_expr(expr: &Expr, constraints: &mut HashMap<String, Vec<LatticeType>>) {
    if let Expr::Var(name, _) = expr {
        if let Some(constraint_list) = constraints.get_mut(name) {
            constraint_list.push(LatticeType::Concrete(ConcreteType::Bool));
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ir::core::{Block, Function, Literal, TypedParam};
    use crate::span::Span;

    fn dummy_span() -> Span {
        Span::new(0, 0, 0, 0, 0, 0)
    }

    fn create_tfuncs() -> TransferFunctions {
        let mut tfuncs = TransferFunctions::new();
        crate::compile::tfuncs::register_all(&mut tfuncs);
        tfuncs
    }

    #[test]
    fn test_infer_numeric_from_addition() {
        let tfuncs = create_tfuncs();

        // function f(x) return x + 1 end
        let func = Function {
            name: "f".to_string(),
            params: vec![TypedParam::new("x".to_string(), None, dummy_span())],
            kwparams: vec![],
            type_params: vec![],
            return_type: None,
            body: Block {
                stmts: vec![Stmt::Return {
                    value: Some(Expr::BinaryOp {
                        op: BinaryOp::Add,
                        left: Box::new(Expr::Var("x".to_string(), dummy_span())),
                        right: Box::new(Expr::Literal(Literal::Int(1), dummy_span())),
                        span: dummy_span(),
                    }),
                    span: dummy_span(),
                }],
                span: dummy_span(),
            },
            is_base_extension: false,
            span: dummy_span(),
        };

        let constraints = infer_parameter_constraints(&func, &tfuncs);

        assert!(constraints.contains_key("x"));
        // Usage analysis should infer Number (abstract numeric type)
        assert_eq!(
            constraints["x"],
            LatticeType::Concrete(ConcreteType::Number),
            "Expected Number constraint for x, got {:?}",
            constraints["x"]
        );
    }

    #[test]
    fn test_infer_integer_from_indexing() {
        let tfuncs = create_tfuncs();

        // function f(arr, i) return arr[i] end
        let func = Function {
            name: "f".to_string(),
            params: vec![
                TypedParam::new("arr".to_string(), None, dummy_span()),
                TypedParam::new("i".to_string(), None, dummy_span()),
            ],
            kwparams: vec![],
            type_params: vec![],
            return_type: None,
            body: Block {
                stmts: vec![Stmt::Return {
                    value: Some(Expr::Index {
                        array: Box::new(Expr::Var("arr".to_string(), dummy_span())),
                        indices: vec![Expr::Var("i".to_string(), dummy_span())],
                        span: dummy_span(),
                    }),
                    span: dummy_span(),
                }],
                span: dummy_span(),
            },
            is_base_extension: false,
            span: dummy_span(),
        };

        let constraints = infer_parameter_constraints(&func, &tfuncs);

        // i should be constrained to Int64 (used as array index)
        assert!(constraints.contains_key("i"));
        assert_eq!(constraints["i"], LatticeType::Concrete(ConcreteType::Int64));
    }

    #[test]
    fn test_infer_bool_from_logical_and() {
        let tfuncs = create_tfuncs();

        // function f(a, b) return a && b end
        let func = Function {
            name: "f".to_string(),
            params: vec![
                TypedParam::new("a".to_string(), None, dummy_span()),
                TypedParam::new("b".to_string(), None, dummy_span()),
            ],
            kwparams: vec![],
            type_params: vec![],
            return_type: None,
            body: Block {
                stmts: vec![Stmt::Return {
                    value: Some(Expr::BinaryOp {
                        op: BinaryOp::And,
                        left: Box::new(Expr::Var("a".to_string(), dummy_span())),
                        right: Box::new(Expr::Var("b".to_string(), dummy_span())),
                        span: dummy_span(),
                    }),
                    span: dummy_span(),
                }],
                span: dummy_span(),
            },
            is_base_extension: false,
            span: dummy_span(),
        };

        let constraints = infer_parameter_constraints(&func, &tfuncs);

        // Both a and b should be constrained to Bool
        assert_eq!(constraints["a"], LatticeType::Concrete(ConcreteType::Bool));
        assert_eq!(constraints["b"], LatticeType::Concrete(ConcreteType::Bool));
    }

    #[test]
    fn test_no_constraint_for_typed_param() {
        let tfuncs = create_tfuncs();

        // function f(x::Int64) return x + 1 end
        let func = Function {
            name: "f".to_string(),
            params: vec![TypedParam::new(
                "x".to_string(),
                Some(crate::types::JuliaType::Int64),
                dummy_span(),
            )],
            kwparams: vec![],
            type_params: vec![],
            return_type: None,
            body: Block {
                stmts: vec![Stmt::Return {
                    value: Some(Expr::BinaryOp {
                        op: BinaryOp::Add,
                        left: Box::new(Expr::Var("x".to_string(), dummy_span())),
                        right: Box::new(Expr::Literal(Literal::Int(1), dummy_span())),
                        span: dummy_span(),
                    }),
                    span: dummy_span(),
                }],
                span: dummy_span(),
            },
            is_base_extension: false,
            span: dummy_span(),
        };

        let constraints = infer_parameter_constraints(&func, &tfuncs);

        // x has a type annotation, so should not appear in constraints
        assert!(!constraints.contains_key("x"));
    }

    #[test]
    fn test_no_constraint_for_unused_param() {
        let tfuncs = create_tfuncs();

        // function f(x) return 42 end
        let func = Function {
            name: "f".to_string(),
            params: vec![TypedParam::new("x".to_string(), None, dummy_span())],
            kwparams: vec![],
            type_params: vec![],
            return_type: None,
            body: Block {
                stmts: vec![Stmt::Return {
                    value: Some(Expr::Literal(Literal::Int(42), dummy_span())),
                    span: dummy_span(),
                }],
                span: dummy_span(),
            },
            is_base_extension: false,
            span: dummy_span(),
        };

        let constraints = infer_parameter_constraints(&func, &tfuncs);

        // x is unused, so should have Top constraint
        assert_eq!(constraints["x"], LatticeType::Top);
    }
}
