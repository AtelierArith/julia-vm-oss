//! Detection of union splitting opportunities.
//!
//! This module identifies conditions where union-typed variables can be
//! split into specialized paths, such as isa checks and nothing checks.

use crate::compile::abstract_interp::TypeEnv;
use crate::compile::lattice::types::{ConcreteType, LatticeType};
use crate::ir::core::{BinaryOp, BuiltinOp, Expr, Function, Literal, Stmt};

/// A candidate for union splitting.
///
/// Represents a condition that tests a union-typed variable and can
/// benefit from splitting execution into specialized paths.
#[derive(Debug, Clone, PartialEq)]
pub struct UnionSplitCandidate {
    /// Name of the variable being tested
    pub var_name: String,
    /// The union type of the variable
    pub union_type: LatticeType,
    /// The splitting condition
    pub condition: SplitCondition,
    /// The conditional expression
    pub condition_expr: Expr,
}

/// Conditions that enable union splitting.
#[derive(Debug, Clone, PartialEq)]
pub enum SplitCondition {
    /// `isa(x, Type)` check
    IsaCheck { target_type: ConcreteType },
    /// `typeof(x) == Type` check (future work)
    TypeofCheck { target_type: ConcreteType },
    /// `x === nothing` or `x !== nothing` check
    NothingCheck { is_equality: bool },
}

impl SplitCondition {
    /// Get the target type for this condition (if applicable).
    pub fn target_type(&self) -> Option<ConcreteType> {
        match self {
            SplitCondition::IsaCheck { target_type } => Some(target_type.clone()),
            SplitCondition::TypeofCheck { target_type } => Some(target_type.clone()),
            SplitCondition::NothingCheck { .. } => Some(ConcreteType::Nothing),
        }
    }
}

/// Find all union split candidates in a function.
///
/// # Arguments
///
/// * `func` - The function to analyze
/// * `env` - The type environment
///
/// # Returns
///
/// A vector of split candidates found in the function.
pub fn find_split_candidates(func: &Function, env: &TypeEnv) -> Vec<UnionSplitCandidate> {
    let mut candidates = Vec::new();
    find_candidates_in_block(&func.body.stmts, env, &mut candidates);
    candidates
}

/// Recursively find split candidates in a block of statements.
fn find_candidates_in_block(
    stmts: &[Stmt],
    env: &TypeEnv,
    candidates: &mut Vec<UnionSplitCandidate>,
) {
    for stmt in stmts {
        find_candidates_in_stmt(stmt, env, candidates);
    }
}

/// Find split candidates in a single statement.
fn find_candidates_in_stmt(stmt: &Stmt, env: &TypeEnv, candidates: &mut Vec<UnionSplitCandidate>) {
    match stmt {
        Stmt::If {
            condition,
            then_branch,
            else_branch,
            ..
        } => {
            // Check if the condition is a split candidate
            if let Some(candidate) = analyze_condition(condition, env) {
                candidates.push(candidate);
            }

            // Recursively check branches
            find_candidates_in_block(&then_branch.stmts, env, candidates);
            if let Some(else_block) = else_branch {
                find_candidates_in_block(&else_block.stmts, env, candidates);
            }
        }
        Stmt::While { body, .. } => {
            find_candidates_in_block(&body.stmts, env, candidates);
        }
        Stmt::For { body, .. } => {
            find_candidates_in_block(&body.stmts, env, candidates);
        }
        Stmt::TestSet { body, .. } => {
            find_candidates_in_block(&body.stmts, env, candidates);
        }
        // Other statement types don't contain conditions
        _ => {}
    }
}

/// Analyze a condition expression to determine if it's a split candidate.
fn analyze_condition(condition: &Expr, env: &TypeEnv) -> Option<UnionSplitCandidate> {
    match condition {
        // isa(x, Type)
        Expr::Builtin {
            name: BuiltinOp::Isa,
            args,
            ..
        } if args.len() == 2 => {
            let var_expr = &args[0];
            let type_expr = &args[1];

            if let (Some(var_name), Some(target_type)) = (
                extract_var_name(var_expr),
                extract_type_from_expr(type_expr),
            ) {
                if let Some(current_type) = env.get(&var_name) {
                    // Only split if the variable is union-typed
                    if is_union_type(current_type) {
                        return Some(UnionSplitCandidate {
                            var_name,
                            union_type: current_type.clone(),
                            condition: SplitCondition::IsaCheck { target_type },
                            condition_expr: condition.clone(),
                        });
                    }
                }
            }
        }

        // x === nothing or x !== nothing
        Expr::BinaryOp {
            op, left, right, ..
        } if matches!(op, BinaryOp::Eq | BinaryOp::Ne) => {
            let is_equality = matches!(op, BinaryOp::Eq);

            // Check if one side is a variable and the other is nothing
            if let Some((var_name, var_type)) = check_nothing_comparison(left, right, env) {
                if is_union_type(&var_type) {
                    return Some(UnionSplitCandidate {
                        var_name,
                        union_type: var_type,
                        condition: SplitCondition::NothingCheck { is_equality },
                        condition_expr: condition.clone(),
                    });
                }
            } else if let Some((var_name, var_type)) = check_nothing_comparison(right, left, env) {
                if is_union_type(&var_type) {
                    return Some(UnionSplitCandidate {
                        var_name,
                        union_type: var_type,
                        condition: SplitCondition::NothingCheck { is_equality },
                        condition_expr: condition.clone(),
                    });
                }
            }
        }

        _ => {}
    }

    None
}

/// Check if a comparison is between a variable and nothing.
fn check_nothing_comparison(
    var_expr: &Expr,
    literal_expr: &Expr,
    env: &TypeEnv,
) -> Option<(String, LatticeType)> {
    if is_nothing_literal(literal_expr) {
        if let Some(var_name) = extract_var_name(var_expr) {
            if let Some(var_type) = env.get(&var_name) {
                return Some((var_name, var_type.clone()));
            }
        }
    }
    None
}

/// Check if an expression is the nothing literal.
fn is_nothing_literal(expr: &Expr) -> bool {
    matches!(expr, Expr::Literal(Literal::Nothing, _))
}

/// Extract a variable name from an expression.
fn extract_var_name(expr: &Expr) -> Option<String> {
    match expr {
        Expr::Var(name, _) => Some(name.clone()),
        _ => None,
    }
}

/// Extract a type from a type expression.
fn extract_type_from_expr(expr: &Expr) -> Option<ConcreteType> {
    match expr {
        Expr::Var(name, _) => match name.as_str() {
            // Signed integers
            "Int8" => Some(ConcreteType::Int8),
            "Int16" => Some(ConcreteType::Int16),
            "Int32" => Some(ConcreteType::Int32),
            "Int" | "Int64" => Some(ConcreteType::Int64),
            "Int128" => Some(ConcreteType::Int128),
            "BigInt" => Some(ConcreteType::BigInt),

            // Unsigned integers
            "UInt8" => Some(ConcreteType::UInt8),
            "UInt16" => Some(ConcreteType::UInt16),
            "UInt32" => Some(ConcreteType::UInt32),
            "UInt" | "UInt64" => Some(ConcreteType::UInt64),
            "UInt128" => Some(ConcreteType::UInt128),

            // Floating point
            "Float32" => Some(ConcreteType::Float32),
            "Float" | "Float64" => Some(ConcreteType::Float64),
            "BigFloat" => Some(ConcreteType::BigFloat),

            // Other types
            "Bool" => Some(ConcreteType::Bool),
            "String" => Some(ConcreteType::String),
            "Char" => Some(ConcreteType::Char),
            "Nothing" => Some(ConcreteType::Nothing),
            "Missing" => Some(ConcreteType::Missing),
            "Symbol" => Some(ConcreteType::Symbol),
            _ => None,
        },
        _ => None,
    }
}

/// Check if a type is a union type (and thus a candidate for splitting).
fn is_union_type(ty: &LatticeType) -> bool {
    matches!(ty, LatticeType::Union(_))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ir::core::Block;
    use crate::span::Span;
    use std::collections::BTreeSet;

    fn dummy_span() -> Span {
        Span::new(0, 0, 0, 0, 0, 0)
    }

    #[test]
    fn test_detect_isa_check() {
        let mut env = TypeEnv::new();
        let mut union_types = BTreeSet::new();
        union_types.insert(ConcreteType::Int64);
        union_types.insert(ConcreteType::String);
        env.set("x", LatticeType::Union(union_types));

        // if x isa Int64
        //     42
        // end
        let func = Function {
            name: "test".to_string(),
            params: vec![],
            kwparams: vec![],
            type_params: vec![],
            return_type: None,
            body: Block {
                stmts: vec![Stmt::If {
                    condition: Expr::Builtin {
                        name: BuiltinOp::Isa,
                        args: vec![
                            Expr::Var("x".to_string(), dummy_span()),
                            Expr::Var("Int64".to_string(), dummy_span()),
                        ],
                        span: dummy_span(),
                    },
                    then_branch: Block {
                        stmts: vec![Stmt::Expr {
                            expr: Expr::Literal(Literal::Int(42), dummy_span()),
                            span: dummy_span(),
                        }],
                        span: dummy_span(),
                    },
                    else_branch: None,
                    span: dummy_span(),
                }],
                span: dummy_span(),
            },
            is_base_extension: false,
            span: dummy_span(),
        };

        let candidates = find_split_candidates(&func, &env);

        assert_eq!(candidates.len(), 1);
        assert_eq!(candidates[0].var_name, "x");
        assert!(matches!(
            candidates[0].condition,
            SplitCondition::IsaCheck { .. }
        ));
    }

    #[test]
    fn test_detect_nothing_check() {
        let mut env = TypeEnv::new();
        let mut union_types = BTreeSet::new();
        union_types.insert(ConcreteType::Int64);
        union_types.insert(ConcreteType::Nothing);
        env.set("x", LatticeType::Union(union_types));

        // if x === nothing
        //     42
        // end
        let func = Function {
            name: "test".to_string(),
            params: vec![],
            kwparams: vec![],
            type_params: vec![],
            return_type: None,
            body: Block {
                stmts: vec![Stmt::If {
                    condition: Expr::BinaryOp {
                        op: BinaryOp::Eq,
                        left: Box::new(Expr::Var("x".to_string(), dummy_span())),
                        right: Box::new(Expr::Literal(Literal::Nothing, dummy_span())),
                        span: dummy_span(),
                    },
                    then_branch: Block {
                        stmts: vec![Stmt::Expr {
                            expr: Expr::Literal(Literal::Int(42), dummy_span()),
                            span: dummy_span(),
                        }],
                        span: dummy_span(),
                    },
                    else_branch: None,
                    span: dummy_span(),
                }],
                span: dummy_span(),
            },
            is_base_extension: false,
            span: dummy_span(),
        };

        let candidates = find_split_candidates(&func, &env);

        assert_eq!(candidates.len(), 1);
        assert_eq!(candidates[0].var_name, "x");
        assert!(matches!(
            candidates[0].condition,
            SplitCondition::NothingCheck { is_equality: true }
        ));
    }

    #[test]
    fn test_no_split_for_concrete_type() {
        let mut env = TypeEnv::new();
        env.set("x", LatticeType::Concrete(ConcreteType::Int64));

        // if x isa Int64 (but x is already Int64, no split needed)
        let func = Function {
            name: "test".to_string(),
            params: vec![],
            kwparams: vec![],
            type_params: vec![],
            return_type: None,
            body: Block {
                stmts: vec![Stmt::If {
                    condition: Expr::Builtin {
                        name: BuiltinOp::Isa,
                        args: vec![
                            Expr::Var("x".to_string(), dummy_span()),
                            Expr::Var("Int64".to_string(), dummy_span()),
                        ],
                        span: dummy_span(),
                    },
                    then_branch: Block {
                        stmts: vec![],
                        span: dummy_span(),
                    },
                    else_branch: None,
                    span: dummy_span(),
                }],
                span: dummy_span(),
            },
            is_base_extension: false,
            span: dummy_span(),
        };

        let candidates = find_split_candidates(&func, &env);

        // Should not create a candidate for concrete types
        assert_eq!(candidates.len(), 0);
    }

    #[test]
    fn test_detect_multiple_candidates() {
        let mut env = TypeEnv::new();
        let mut union_types1 = BTreeSet::new();
        union_types1.insert(ConcreteType::Int64);
        union_types1.insert(ConcreteType::String);
        env.set("x", LatticeType::Union(union_types1));

        let mut union_types2 = BTreeSet::new();
        union_types2.insert(ConcreteType::Float64);
        union_types2.insert(ConcreteType::Nothing);
        env.set("y", LatticeType::Union(union_types2));

        // if x isa Int64
        //     if y === nothing
        //         42
        //     end
        // end
        let func = Function {
            name: "test".to_string(),
            params: vec![],
            kwparams: vec![],
            type_params: vec![],
            return_type: None,
            body: Block {
                stmts: vec![Stmt::If {
                    condition: Expr::Builtin {
                        name: BuiltinOp::Isa,
                        args: vec![
                            Expr::Var("x".to_string(), dummy_span()),
                            Expr::Var("Int64".to_string(), dummy_span()),
                        ],
                        span: dummy_span(),
                    },
                    then_branch: Block {
                        stmts: vec![Stmt::If {
                            condition: Expr::BinaryOp {
                                op: BinaryOp::Eq,
                                left: Box::new(Expr::Var("y".to_string(), dummy_span())),
                                right: Box::new(Expr::Literal(Literal::Nothing, dummy_span())),
                                span: dummy_span(),
                            },
                            then_branch: Block {
                                stmts: vec![Stmt::Expr {
                                    expr: Expr::Literal(Literal::Int(42), dummy_span()),
                                    span: dummy_span(),
                                }],
                                span: dummy_span(),
                            },
                            else_branch: None,
                            span: dummy_span(),
                        }],
                        span: dummy_span(),
                    },
                    else_branch: None,
                    span: dummy_span(),
                }],
                span: dummy_span(),
            },
            is_base_extension: false,
            span: dummy_span(),
        };

        let candidates = find_split_candidates(&func, &env);

        // Should detect both candidates
        assert_eq!(candidates.len(), 2);
    }
}
