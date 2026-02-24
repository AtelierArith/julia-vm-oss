//! Conditional type narrowing for abstract interpretation.
//!
//! This module implements type narrowing in conditional branches,
//! enabling more precise type inference through control flow analysis.
//!
//! # Implementation Approach
//!
//! This module uses **environment splitting** rather than generating Conditional types
//! in the lattice. This approach:
//!
//! - **Splits the type environment** into separate environments for then/else branches
//! - **Applies type narrowing** to each branch's environment
//! - **Merges environments** after analyzing both branches
//!
//! This is functionally equivalent to using Conditional types but simpler to implement
//! and maintain. The design document (TYPE_INFERENCE_ENHANCEMENT.md) describes Conditional
//! types, but the actual implementation uses environment splitting for practical reasons.
//!
//! # Design Trade-offs
//!
//! **Environment Splitting (Current Implementation)**:
//! - ✅ Simpler implementation
//! - ✅ Works correctly for all tested cases
//! - ✅ Lower maintenance burden
//! - ❌ Conditional information is lost after merge
//! - ❌ Cannot represent conditional types in function signatures
//!
//! **Conditional Types (Design Specification)**:
//! - ✅ Preserves conditional information in the lattice
//! - ✅ Better optimization potential
//! - ✅ Consistent with Julia's approach
//! - ❌ More complex implementation
//! - ❌ Requires updates to all lattice operations
//!
//! The current implementation prioritizes correctness and simplicity over perfect
//! alignment with the design specification. Conditional types could be added as
//! a future enhancement if optimization becomes critical.

use crate::compile::abstract_interp::TypeEnv;
use crate::compile::lattice::types::{ConcreteType, LatticeType};
use crate::ir::core::{BinaryOp, BuiltinOp, Expr, Literal, UnaryOp};

/// Result of splitting an environment by a condition.
#[derive(Debug)]
pub struct SplitEnv {
    /// Environment for the then-branch (condition is true)
    pub then_env: TypeEnv,
    /// Environment for the else-branch (condition is false)
    pub else_env: TypeEnv,
}

/// Splits an environment based on a conditional expression.
///
/// This function analyzes the condition and narrows types accordingly:
/// - `isa(val, Type)`: Narrows `val` to `Type` in then-branch
/// - `val === nothing`: Narrows `val` to `Nothing` in then-branch
/// - `val !== nothing`: Narrows `val` to exclude `Nothing` in then-branch
///
/// # Example
/// ```text
/// if val isa Int
///     # then-branch: val is Int
/// else
///     # else-branch: val is not Int (uses subtract)
/// end
/// ```
pub fn split_env_by_condition(env: &TypeEnv, condition: &Expr) -> SplitEnv {
    match condition {
        // isa(val, Type) pattern
        Expr::Call { function, args, .. } if function == "isa" && args.len() == 2 => {
            handle_isa_condition(env, &args[0], &args[1])
        }

        // isa builtin: isa(val, Type)
        Expr::Builtin { name, args, .. } if matches!(name, BuiltinOp::Isa) && args.len() == 2 => {
            handle_isa_condition(env, &args[0], &args[1])
        }

        // val === nothing (using === operator - Egal)
        Expr::BinaryOp {
            op, left, right, ..
        } if matches!(op, BinaryOp::Egal) && is_nothing_literal(right) => {
            handle_nothing_check(env, left, true)
        }

        // nothing === val (using === operator - Egal)
        Expr::BinaryOp {
            op, left, right, ..
        } if matches!(op, BinaryOp::Egal) && is_nothing_literal(left) => {
            handle_nothing_check(env, right, true)
        }

        // val !== nothing (using !== operator - NotEgal)
        Expr::BinaryOp {
            op, left, right, ..
        } if matches!(op, BinaryOp::NotEgal) && is_nothing_literal(right) => {
            handle_nothing_check(env, left, false)
        }

        // nothing !== val (using !== operator - NotEgal)
        Expr::BinaryOp {
            op, left, right, ..
        } if matches!(op, BinaryOp::NotEgal) && is_nothing_literal(left) => {
            handle_nothing_check(env, right, false)
        }

        // Also handle == and != for backwards compatibility
        // val == nothing (using == operator - Eq)
        Expr::BinaryOp {
            op, left, right, ..
        } if matches!(op, BinaryOp::Eq) && is_nothing_literal(right) => {
            handle_nothing_check(env, left, true)
        }

        // nothing == val (using == operator - Eq)
        Expr::BinaryOp {
            op, left, right, ..
        } if matches!(op, BinaryOp::Eq) && is_nothing_literal(left) => {
            handle_nothing_check(env, right, true)
        }

        // val != nothing (using != operator - Ne)
        Expr::BinaryOp {
            op, left, right, ..
        } if matches!(op, BinaryOp::Ne) && is_nothing_literal(right) => {
            handle_nothing_check(env, left, false)
        }

        // nothing != val (using != operator - Ne)
        Expr::BinaryOp {
            op, left, right, ..
        } if matches!(op, BinaryOp::Ne) && is_nothing_literal(left) => {
            handle_nothing_check(env, right, false)
        }

        // !cond (logical NOT) - swap then and else branches
        Expr::UnaryOp {
            op: UnaryOp::Not,
            operand,
            ..
        } => {
            let inner_split = split_env_by_condition(env, operand);
            // Negate: swap then and else environments
            SplitEnv {
                then_env: inner_split.else_env,
                else_env: inner_split.then_env,
            }
        }

        // cond1 && cond2 (logical AND)
        // then: both conditions are true (apply both narrowings)
        // else: at least one condition is false (join of possible false cases)
        Expr::BinaryOp {
            op: BinaryOp::And,
            left,
            right,
            ..
        } => {
            // First, split by cond1
            let split1 = split_env_by_condition(env, left);
            // Then, split cond1's then-branch by cond2
            let split2 = split_env_by_condition(&split1.then_env, right);

            // then: cond1 true AND cond2 true
            let then_env = split2.then_env;

            // else: cond1 false OR (cond1 true AND cond2 false)
            // This is the join of split1.else_env and split2.else_env
            let mut else_env = split1.else_env.clone();
            else_env.merge(&split2.else_env);

            SplitEnv { then_env, else_env }
        }

        // cond1 || cond2 (logical OR)
        // then: at least one condition is true (join of possible true cases)
        // else: both conditions are false (apply both narrowings)
        Expr::BinaryOp {
            op: BinaryOp::Or,
            left,
            right,
            ..
        } => {
            // First, split by cond1
            let split1 = split_env_by_condition(env, left);
            // Then, split cond1's else-branch by cond2
            let split2 = split_env_by_condition(&split1.else_env, right);

            // then: cond1 true OR (cond1 false AND cond2 true)
            // This is the join of split1.then_env and split2.then_env
            let mut then_env = split1.then_env.clone();
            then_env.merge(&split2.then_env);

            // else: cond1 false AND cond2 false
            let else_env = split2.else_env;

            SplitEnv { then_env, else_env }
        }

        // Unhandled condition: no narrowing
        _ => SplitEnv {
            then_env: env.clone(),
            else_env: env.clone(),
        },
    }
}

/// Extracts a narrowable path from an expression.
///
/// This handles:
/// - `Expr::Var(name)` -> `Some("name")`
/// - `Expr::FieldAccess { object: Expr::Var(obj), field }` -> `Some("obj.field")`
/// - `Expr::Index { array: Expr::Var(arr), indices: [Literal(i)] }` -> `Some("arr[i]")`
///
/// Returns `None` for complex expressions that cannot be tracked in the environment.
fn extract_narrowable_path(expr: &Expr) -> Option<String> {
    match expr {
        // Simple variable
        Expr::Var(name, _) => Some(name.clone()),

        // Field access: obj.field
        Expr::FieldAccess { object, field, .. } => {
            // Only supports single-level field access (nested a.b.c not yet supported)
            if let Expr::Var(obj_name, _) = object.as_ref() {
                Some(format!("{}.{}", obj_name, field))
            } else {
                // Nested field access like a.b.c - could be supported in the future
                None
            }
        }

        // Index access: arr[i] where i is a constant
        Expr::Index { array, indices, .. } => {
            // Only support single-level indexing on variables with constant indices
            if let Expr::Var(arr_name, _) = array.as_ref() {
                if indices.len() == 1 {
                    // Extract constant index
                    if let Some(idx_str) = extract_constant_index(&indices[0]) {
                        return Some(format!("{}[{}]", arr_name, idx_str));
                    }
                }
            }
            None
        }

        _ => None,
    }
}

/// Extracts a constant index value as a string.
fn extract_constant_index(expr: &Expr) -> Option<String> {
    match expr {
        Expr::Literal(Literal::Int(i), _) => Some(i.to_string()),
        Expr::Literal(Literal::Bool(b), _) => Some(if *b { "true" } else { "false" }.to_string()),
        // Could extend to support Symbol literals in the future
        _ => None,
    }
}

/// Handles `isa(val, Type)` conditional narrowing.
fn handle_isa_condition(env: &TypeEnv, val_expr: &Expr, type_expr: &Expr) -> SplitEnv {
    // Extract narrowable path from val_expr (supports Var, FieldAccess, Index)
    let path = match extract_narrowable_path(val_expr) {
        Some(p) => p,
        None => {
            // Complex expression: cannot narrow
            return SplitEnv {
                then_env: env.clone(),
                else_env: env.clone(),
            };
        }
    };

    // Extract target type from type_expr
    let target_type = match extract_type_from_expr(type_expr) {
        Some(ty) => ty,
        None => {
            // Cannot determine type: no narrowing
            return SplitEnv {
                then_env: env.clone(),
                else_env: env.clone(),
            };
        }
    };

    // Get current type of the path
    let current_type = match env.get(&path) {
        Some(ty) => ty.clone(),
        None => {
            // Path not in environment: no narrowing
            return SplitEnv {
                then_env: env.clone(),
                else_env: env.clone(),
            };
        }
    };

    // Then-branch: narrow to target type (intersection)
    let then_type = current_type.meet(&target_type);
    let mut then_env = env.clone();
    then_env.set(&path, then_type);

    // Else-branch: exclude target type (subtraction)
    let else_type = current_type.subtract(&target_type);
    let mut else_env = env.clone();
    else_env.set(&path, else_type);

    SplitEnv { then_env, else_env }
}

/// Handles `val === nothing` or `val !== nothing` conditional narrowing.
fn handle_nothing_check(env: &TypeEnv, val_expr: &Expr, is_equality: bool) -> SplitEnv {
    // Extract narrowable path (supports Var, FieldAccess, Index)
    let path = match extract_narrowable_path(val_expr) {
        Some(p) => p,
        None => {
            // Complex expression: cannot narrow
            return SplitEnv {
                then_env: env.clone(),
                else_env: env.clone(),
            };
        }
    };

    // Get current type of the path
    let current_type = match env.get(&path) {
        Some(ty) => ty.clone(),
        None => {
            // Path not in environment: no narrowing
            return SplitEnv {
                then_env: env.clone(),
                else_env: env.clone(),
            };
        }
    };

    let nothing_type = LatticeType::Concrete(ConcreteType::Nothing);

    let (then_type, else_type) = if is_equality {
        // val === nothing:
        // - then-branch: val is Nothing
        // - else-branch: val is not Nothing
        (
            current_type.meet(&nothing_type),
            current_type.subtract(&nothing_type),
        )
    } else {
        // val !== nothing:
        // - then-branch: val is not Nothing
        // - else-branch: val is Nothing
        (
            current_type.subtract(&nothing_type),
            current_type.meet(&nothing_type),
        )
    };

    let mut then_env = env.clone();
    then_env.set(&path, then_type);

    let mut else_env = env.clone();
    else_env.set(&path, else_type);

    SplitEnv { then_env, else_env }
}

/// Checks if an expression is the `nothing` literal.
fn is_nothing_literal(expr: &Expr) -> bool {
    matches!(expr, Expr::Literal(Literal::Nothing, _))
}

/// Extracts a LatticeType from a type expression.
///
/// This handles simple type expressions like:
/// - Variable names: `Int`, `Float64`, `String`
/// - All numeric types: Int8, Int16, Int32, Int64, Int128, UInt8-UInt128, Float32, Float64
/// - Type constructors (future work)
fn extract_type_from_expr(expr: &Expr) -> Option<LatticeType> {
    match expr {
        // Simple type name as variable
        Expr::Var(name, _) => match name.as_str() {
            // Signed integers
            "Int8" => Some(LatticeType::Concrete(ConcreteType::Int8)),
            "Int16" => Some(LatticeType::Concrete(ConcreteType::Int16)),
            "Int32" => Some(LatticeType::Concrete(ConcreteType::Int32)),
            "Int" | "Int64" => Some(LatticeType::Concrete(ConcreteType::Int64)),
            "Int128" => Some(LatticeType::Concrete(ConcreteType::Int128)),
            "BigInt" => Some(LatticeType::Concrete(ConcreteType::BigInt)),

            // Unsigned integers
            "UInt8" => Some(LatticeType::Concrete(ConcreteType::UInt8)),
            "UInt16" => Some(LatticeType::Concrete(ConcreteType::UInt16)),
            "UInt32" => Some(LatticeType::Concrete(ConcreteType::UInt32)),
            "UInt" | "UInt64" => Some(LatticeType::Concrete(ConcreteType::UInt64)),
            "UInt128" => Some(LatticeType::Concrete(ConcreteType::UInt128)),

            // Floating point
            "Float32" => Some(LatticeType::Concrete(ConcreteType::Float32)),
            "Float" | "Float64" => Some(LatticeType::Concrete(ConcreteType::Float64)),
            "BigFloat" => Some(LatticeType::Concrete(ConcreteType::BigFloat)),

            // Other types
            "Bool" => Some(LatticeType::Concrete(ConcreteType::Bool)),
            "String" => Some(LatticeType::Concrete(ConcreteType::String)),
            "Char" => Some(LatticeType::Concrete(ConcreteType::Char)),
            "Nothing" => Some(LatticeType::Concrete(ConcreteType::Nothing)),
            "Missing" => Some(LatticeType::Concrete(ConcreteType::Missing)),
            "Symbol" => Some(LatticeType::Concrete(ConcreteType::Symbol)),
            _ => None,
        },

        // Type literal (if supported)
        _ => None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::span::Span;

    fn dummy_span() -> Span {
        Span::new(0, 0, 0, 0, 0, 0)
    }

    #[test]
    fn test_split_env_isa_narrows_then_branch() {
        let mut env = TypeEnv::new();
        // val has type Any (Top)
        env.set("val", LatticeType::Top);

        // Condition: isa(val, Int64)
        let condition = Expr::Builtin {
            name: BuiltinOp::Isa,
            args: vec![
                Expr::Var("val".to_string(), dummy_span()),
                Expr::Var("Int64".to_string(), dummy_span()),
            ],
            span: dummy_span(),
        };

        let split = split_env_by_condition(&env, &condition);

        // Then-branch: val should be Int64
        assert_eq!(
            split.then_env.get("val"),
            Some(&LatticeType::Concrete(ConcreteType::Int64))
        );

        // Else-branch: val should be Top (can't subtract from Top)
        assert_eq!(split.else_env.get("val"), Some(&LatticeType::Top));
    }

    #[test]
    fn test_split_env_isa_with_union() {
        let mut env = TypeEnv::new();
        // val has type Union{Int64, String}
        let mut union_types = std::collections::BTreeSet::new();
        union_types.insert(ConcreteType::Int64);
        union_types.insert(ConcreteType::String);
        env.set("val", LatticeType::Union(union_types));

        // Condition: isa(val, Int64)
        let condition = Expr::Builtin {
            name: BuiltinOp::Isa,
            args: vec![
                Expr::Var("val".to_string(), dummy_span()),
                Expr::Var("Int64".to_string(), dummy_span()),
            ],
            span: dummy_span(),
        };

        let split = split_env_by_condition(&env, &condition);

        // Then-branch: val should be Int64
        assert_eq!(
            split.then_env.get("val"),
            Some(&LatticeType::Concrete(ConcreteType::Int64))
        );

        // Else-branch: val should be String (Union - Int64)
        assert_eq!(
            split.else_env.get("val"),
            Some(&LatticeType::Concrete(ConcreteType::String))
        );
    }

    #[test]
    fn test_split_env_nothing_check_equality() {
        let mut env = TypeEnv::new();
        // val has type Union{Int64, Nothing}
        let mut union_types = std::collections::BTreeSet::new();
        union_types.insert(ConcreteType::Int64);
        union_types.insert(ConcreteType::Nothing);
        env.set("val", LatticeType::Union(union_types));

        // Condition: val === nothing (uses Egal operator)
        let condition = Expr::BinaryOp {
            op: BinaryOp::Egal,
            left: Box::new(Expr::Var("val".to_string(), dummy_span())),
            right: Box::new(Expr::Literal(Literal::Nothing, dummy_span())),
            span: dummy_span(),
        };

        let split = split_env_by_condition(&env, &condition);

        // Then-branch: val should be Nothing
        assert_eq!(
            split.then_env.get("val"),
            Some(&LatticeType::Concrete(ConcreteType::Nothing))
        );

        // Else-branch: val should be Int64 (Union - Nothing)
        assert_eq!(
            split.else_env.get("val"),
            Some(&LatticeType::Concrete(ConcreteType::Int64))
        );
    }

    #[test]
    fn test_split_env_nothing_check_inequality() {
        let mut env = TypeEnv::new();
        // val has type Union{String, Nothing}
        let mut union_types = std::collections::BTreeSet::new();
        union_types.insert(ConcreteType::String);
        union_types.insert(ConcreteType::Nothing);
        env.set("val", LatticeType::Union(union_types));

        // Condition: val !== nothing (uses NotEgal operator)
        let condition = Expr::BinaryOp {
            op: BinaryOp::NotEgal,
            left: Box::new(Expr::Var("val".to_string(), dummy_span())),
            right: Box::new(Expr::Literal(Literal::Nothing, dummy_span())),
            span: dummy_span(),
        };

        let split = split_env_by_condition(&env, &condition);

        // Then-branch: val should be String (not nothing)
        assert_eq!(
            split.then_env.get("val"),
            Some(&LatticeType::Concrete(ConcreteType::String))
        );

        // Else-branch: val should be Nothing
        assert_eq!(
            split.else_env.get("val"),
            Some(&LatticeType::Concrete(ConcreteType::Nothing))
        );
    }

    #[test]
    fn test_split_env_unhandled_condition() {
        let mut env = TypeEnv::new();
        env.set("x", LatticeType::Concrete(ConcreteType::Int64));

        // Condition: x > 5 (not a type narrowing condition)
        let condition = Expr::BinaryOp {
            op: BinaryOp::Gt,
            left: Box::new(Expr::Var("x".to_string(), dummy_span())),
            right: Box::new(Expr::Literal(Literal::Int(5), dummy_span())),
            span: dummy_span(),
        };

        let split = split_env_by_condition(&env, &condition);

        // Both branches should have the same environment (no narrowing)
        assert_eq!(
            split.then_env.get("x"),
            Some(&LatticeType::Concrete(ConcreteType::Int64))
        );
        assert_eq!(
            split.else_env.get("x"),
            Some(&LatticeType::Concrete(ConcreteType::Int64))
        );
    }

    #[test]
    fn test_is_nothing_literal() {
        assert!(is_nothing_literal(&Expr::Literal(
            Literal::Nothing,
            dummy_span()
        )));
        assert!(!is_nothing_literal(&Expr::Literal(
            Literal::Int(42),
            dummy_span()
        )));
        assert!(!is_nothing_literal(&Expr::Var(
            "x".to_string(),
            dummy_span()
        )));
    }

    #[test]
    fn test_extract_type_from_expr() {
        // Basic types
        assert_eq!(
            extract_type_from_expr(&Expr::Var("Int64".to_string(), dummy_span())),
            Some(LatticeType::Concrete(ConcreteType::Int64))
        );
        assert_eq!(
            extract_type_from_expr(&Expr::Var("Float64".to_string(), dummy_span())),
            Some(LatticeType::Concrete(ConcreteType::Float64))
        );
        assert_eq!(
            extract_type_from_expr(&Expr::Var("String".to_string(), dummy_span())),
            Some(LatticeType::Concrete(ConcreteType::String))
        );
        assert_eq!(
            extract_type_from_expr(&Expr::Var("UnknownType".to_string(), dummy_span())),
            None
        );
    }

    #[test]
    fn test_extract_type_from_expr_all_numeric_types() {
        // Signed integers
        assert_eq!(
            extract_type_from_expr(&Expr::Var("Int8".to_string(), dummy_span())),
            Some(LatticeType::Concrete(ConcreteType::Int8))
        );
        assert_eq!(
            extract_type_from_expr(&Expr::Var("Int16".to_string(), dummy_span())),
            Some(LatticeType::Concrete(ConcreteType::Int16))
        );
        assert_eq!(
            extract_type_from_expr(&Expr::Var("Int32".to_string(), dummy_span())),
            Some(LatticeType::Concrete(ConcreteType::Int32))
        );
        assert_eq!(
            extract_type_from_expr(&Expr::Var("Int".to_string(), dummy_span())),
            Some(LatticeType::Concrete(ConcreteType::Int64))
        );
        assert_eq!(
            extract_type_from_expr(&Expr::Var("Int128".to_string(), dummy_span())),
            Some(LatticeType::Concrete(ConcreteType::Int128))
        );
        assert_eq!(
            extract_type_from_expr(&Expr::Var("BigInt".to_string(), dummy_span())),
            Some(LatticeType::Concrete(ConcreteType::BigInt))
        );

        // Unsigned integers
        assert_eq!(
            extract_type_from_expr(&Expr::Var("UInt8".to_string(), dummy_span())),
            Some(LatticeType::Concrete(ConcreteType::UInt8))
        );
        assert_eq!(
            extract_type_from_expr(&Expr::Var("UInt16".to_string(), dummy_span())),
            Some(LatticeType::Concrete(ConcreteType::UInt16))
        );
        assert_eq!(
            extract_type_from_expr(&Expr::Var("UInt32".to_string(), dummy_span())),
            Some(LatticeType::Concrete(ConcreteType::UInt32))
        );
        assert_eq!(
            extract_type_from_expr(&Expr::Var("UInt".to_string(), dummy_span())),
            Some(LatticeType::Concrete(ConcreteType::UInt64))
        );
        assert_eq!(
            extract_type_from_expr(&Expr::Var("UInt64".to_string(), dummy_span())),
            Some(LatticeType::Concrete(ConcreteType::UInt64))
        );
        assert_eq!(
            extract_type_from_expr(&Expr::Var("UInt128".to_string(), dummy_span())),
            Some(LatticeType::Concrete(ConcreteType::UInt128))
        );

        // Floating point
        assert_eq!(
            extract_type_from_expr(&Expr::Var("Float32".to_string(), dummy_span())),
            Some(LatticeType::Concrete(ConcreteType::Float32))
        );
        assert_eq!(
            extract_type_from_expr(&Expr::Var("Float".to_string(), dummy_span())),
            Some(LatticeType::Concrete(ConcreteType::Float64))
        );
        assert_eq!(
            extract_type_from_expr(&Expr::Var("BigFloat".to_string(), dummy_span())),
            Some(LatticeType::Concrete(ConcreteType::BigFloat))
        );

        // Other types
        assert_eq!(
            extract_type_from_expr(&Expr::Var("Bool".to_string(), dummy_span())),
            Some(LatticeType::Concrete(ConcreteType::Bool))
        );
        assert_eq!(
            extract_type_from_expr(&Expr::Var("Char".to_string(), dummy_span())),
            Some(LatticeType::Concrete(ConcreteType::Char))
        );
        assert_eq!(
            extract_type_from_expr(&Expr::Var("Nothing".to_string(), dummy_span())),
            Some(LatticeType::Concrete(ConcreteType::Nothing))
        );
        assert_eq!(
            extract_type_from_expr(&Expr::Var("Missing".to_string(), dummy_span())),
            Some(LatticeType::Concrete(ConcreteType::Missing))
        );
        assert_eq!(
            extract_type_from_expr(&Expr::Var("Symbol".to_string(), dummy_span())),
            Some(LatticeType::Concrete(ConcreteType::Symbol))
        );
    }

    // ====== Tests for compound boolean conditions (&&, ||, !) ======

    #[test]
    fn test_split_env_not_operator() {
        let mut env = TypeEnv::new();
        // val has type Union{Int64, Nothing}
        let mut union_types = std::collections::BTreeSet::new();
        union_types.insert(ConcreteType::Int64);
        union_types.insert(ConcreteType::Nothing);
        env.set("val", LatticeType::Union(union_types));

        // Condition: !(val === nothing)
        // This is equivalent to: val !== nothing
        let inner_condition = Expr::BinaryOp {
            op: BinaryOp::Egal,
            left: Box::new(Expr::Var("val".to_string(), dummy_span())),
            right: Box::new(Expr::Literal(Literal::Nothing, dummy_span())),
            span: dummy_span(),
        };
        let condition = Expr::UnaryOp {
            op: UnaryOp::Not,
            operand: Box::new(inner_condition),
            span: dummy_span(),
        };

        let split = split_env_by_condition(&env, &condition);

        // Then-branch: val should be Int64 (NOT nothing)
        assert_eq!(
            split.then_env.get("val"),
            Some(&LatticeType::Concrete(ConcreteType::Int64))
        );

        // Else-branch: val should be Nothing
        assert_eq!(
            split.else_env.get("val"),
            Some(&LatticeType::Concrete(ConcreteType::Nothing))
        );
    }

    #[test]
    fn test_split_env_and_operator_both_narrow() {
        let mut env = TypeEnv::new();
        // val has type Union{Int64, String, Nothing}
        let mut union_types = std::collections::BTreeSet::new();
        union_types.insert(ConcreteType::Int64);
        union_types.insert(ConcreteType::String);
        union_types.insert(ConcreteType::Nothing);
        env.set("val", LatticeType::Union(union_types));

        // Condition: val !== nothing && isa(val, Int64)
        // After &&: val should be Int64 in then-branch
        let cond1 = Expr::BinaryOp {
            op: BinaryOp::NotEgal,
            left: Box::new(Expr::Var("val".to_string(), dummy_span())),
            right: Box::new(Expr::Literal(Literal::Nothing, dummy_span())),
            span: dummy_span(),
        };
        let cond2 = Expr::Builtin {
            name: BuiltinOp::Isa,
            args: vec![
                Expr::Var("val".to_string(), dummy_span()),
                Expr::Var("Int64".to_string(), dummy_span()),
            ],
            span: dummy_span(),
        };
        let condition = Expr::BinaryOp {
            op: BinaryOp::And,
            left: Box::new(cond1),
            right: Box::new(cond2),
            span: dummy_span(),
        };

        let split = split_env_by_condition(&env, &condition);

        // Then-branch: val should be Int64 (both conditions satisfied)
        assert_eq!(
            split.then_env.get("val"),
            Some(&LatticeType::Concrete(ConcreteType::Int64))
        );

        // Else-branch: val could be Nothing or String (not narrowed to a single type)
        // The else branch is the join of:
        // - cond1 false (val is Nothing)
        // - cond1 true but cond2 false (val is String)
        // Result should be Union{Nothing, String}
        let else_type = split.else_env.get("val").unwrap();
        assert!(
            matches!(else_type, LatticeType::Union(_)),
            "Expected Union type, got {:?}",
            else_type
        );
        if let LatticeType::Union(types) = else_type {
            assert!(types.contains(&ConcreteType::Nothing));
            assert!(types.contains(&ConcreteType::String));
            assert!(!types.contains(&ConcreteType::Int64));
        }
    }

    #[test]
    fn test_split_env_or_operator_either_narrow() {
        let mut env = TypeEnv::new();
        // val has type Union{Int64, String, Bool}
        let mut union_types = std::collections::BTreeSet::new();
        union_types.insert(ConcreteType::Int64);
        union_types.insert(ConcreteType::String);
        union_types.insert(ConcreteType::Bool);
        env.set("val", LatticeType::Union(union_types));

        // Condition: isa(val, Int64) || isa(val, String)
        // Then-branch: val could be Int64 or String
        // Else-branch: val should be Bool
        let cond1 = Expr::Builtin {
            name: BuiltinOp::Isa,
            args: vec![
                Expr::Var("val".to_string(), dummy_span()),
                Expr::Var("Int64".to_string(), dummy_span()),
            ],
            span: dummy_span(),
        };
        let cond2 = Expr::Builtin {
            name: BuiltinOp::Isa,
            args: vec![
                Expr::Var("val".to_string(), dummy_span()),
                Expr::Var("String".to_string(), dummy_span()),
            ],
            span: dummy_span(),
        };
        let condition = Expr::BinaryOp {
            op: BinaryOp::Or,
            left: Box::new(cond1),
            right: Box::new(cond2),
            span: dummy_span(),
        };

        let split = split_env_by_condition(&env, &condition);

        // Then-branch: val could be Int64 or String (join of both true branches)
        let then_type = split.then_env.get("val").unwrap();
        assert!(
            matches!(then_type, LatticeType::Union(_)),
            "Expected Union type, got {:?}",
            then_type
        );
        if let LatticeType::Union(types) = then_type {
            assert!(types.contains(&ConcreteType::Int64));
            assert!(types.contains(&ConcreteType::String));
        }

        // Else-branch: val should be Bool (both conditions false)
        assert_eq!(
            split.else_env.get("val"),
            Some(&LatticeType::Concrete(ConcreteType::Bool))
        );
    }

    #[test]
    fn test_split_env_nested_not_not() {
        let mut env = TypeEnv::new();
        // val has type Union{Int64, Nothing}
        let mut union_types = std::collections::BTreeSet::new();
        union_types.insert(ConcreteType::Int64);
        union_types.insert(ConcreteType::Nothing);
        env.set("val", LatticeType::Union(union_types));

        // Condition: !!(val === nothing)
        // Double negation should give same result as val === nothing
        let inner_condition = Expr::BinaryOp {
            op: BinaryOp::Egal,
            left: Box::new(Expr::Var("val".to_string(), dummy_span())),
            right: Box::new(Expr::Literal(Literal::Nothing, dummy_span())),
            span: dummy_span(),
        };
        let not_condition = Expr::UnaryOp {
            op: UnaryOp::Not,
            operand: Box::new(inner_condition),
            span: dummy_span(),
        };
        let condition = Expr::UnaryOp {
            op: UnaryOp::Not,
            operand: Box::new(not_condition),
            span: dummy_span(),
        };

        let split = split_env_by_condition(&env, &condition);

        // Then-branch: val should be Nothing (same as val === nothing)
        assert_eq!(
            split.then_env.get("val"),
            Some(&LatticeType::Concrete(ConcreteType::Nothing))
        );

        // Else-branch: val should be Int64
        assert_eq!(
            split.else_env.get("val"),
            Some(&LatticeType::Concrete(ConcreteType::Int64))
        );
    }

    #[test]
    fn test_split_env_not_isa() {
        let mut env = TypeEnv::new();
        // val has type Union{Int64, String}
        let mut union_types = std::collections::BTreeSet::new();
        union_types.insert(ConcreteType::Int64);
        union_types.insert(ConcreteType::String);
        env.set("val", LatticeType::Union(union_types));

        // Condition: !(val isa Int64)
        // then: val is NOT Int64 → String
        // else: val is Int64
        let isa_condition = Expr::Builtin {
            name: BuiltinOp::Isa,
            args: vec![
                Expr::Var("val".to_string(), dummy_span()),
                Expr::Var("Int64".to_string(), dummy_span()),
            ],
            span: dummy_span(),
        };
        let condition = Expr::UnaryOp {
            op: UnaryOp::Not,
            operand: Box::new(isa_condition),
            span: dummy_span(),
        };

        let split = split_env_by_condition(&env, &condition);

        // Then-branch: val should be String (NOT Int64)
        assert_eq!(
            split.then_env.get("val"),
            Some(&LatticeType::Concrete(ConcreteType::String))
        );

        // Else-branch: val should be Int64
        assert_eq!(
            split.else_env.get("val"),
            Some(&LatticeType::Concrete(ConcreteType::Int64))
        );
    }

    // ====== Tests for field/index access type narrowing (Issue #1641) ======

    #[test]
    fn test_extract_narrowable_path_var() {
        let expr = Expr::Var("x".to_string(), dummy_span());
        assert_eq!(extract_narrowable_path(&expr), Some("x".to_string()));
    }

    #[test]
    fn test_extract_narrowable_path_field_access() {
        let expr = Expr::FieldAccess {
            object: Box::new(Expr::Var("obj".to_string(), dummy_span())),
            field: "field".to_string(),
            span: dummy_span(),
        };
        assert_eq!(
            extract_narrowable_path(&expr),
            Some("obj.field".to_string())
        );
    }

    #[test]
    fn test_extract_narrowable_path_index() {
        let expr = Expr::Index {
            array: Box::new(Expr::Var("arr".to_string(), dummy_span())),
            indices: vec![Expr::Literal(Literal::Int(1), dummy_span())],
            span: dummy_span(),
        };
        assert_eq!(extract_narrowable_path(&expr), Some("arr[1]".to_string()));
    }

    #[test]
    fn test_extract_narrowable_path_nested_field_returns_none() {
        // a.b.c should return None (not supported yet)
        let inner = Expr::FieldAccess {
            object: Box::new(Expr::Var("a".to_string(), dummy_span())),
            field: "b".to_string(),
            span: dummy_span(),
        };
        let expr = Expr::FieldAccess {
            object: Box::new(inner),
            field: "c".to_string(),
            span: dummy_span(),
        };
        assert_eq!(extract_narrowable_path(&expr), None);
    }

    #[test]
    fn test_split_env_isa_field_access() {
        let mut env = TypeEnv::new();
        // obj.field has type Union{Int64, Nothing}
        let mut union_types = std::collections::BTreeSet::new();
        union_types.insert(ConcreteType::Int64);
        union_types.insert(ConcreteType::Nothing);
        env.set("obj.field", LatticeType::Union(union_types));

        // Condition: isa(obj.field, Int64)
        let condition = Expr::Builtin {
            name: BuiltinOp::Isa,
            args: vec![
                Expr::FieldAccess {
                    object: Box::new(Expr::Var("obj".to_string(), dummy_span())),
                    field: "field".to_string(),
                    span: dummy_span(),
                },
                Expr::Var("Int64".to_string(), dummy_span()),
            ],
            span: dummy_span(),
        };

        let split = split_env_by_condition(&env, &condition);

        // Then-branch: obj.field should be Int64
        assert_eq!(
            split.then_env.get("obj.field"),
            Some(&LatticeType::Concrete(ConcreteType::Int64))
        );

        // Else-branch: obj.field should be Nothing
        assert_eq!(
            split.else_env.get("obj.field"),
            Some(&LatticeType::Concrete(ConcreteType::Nothing))
        );
    }

    #[test]
    fn test_split_env_nothing_check_field_access() {
        let mut env = TypeEnv::new();
        // obj.value has type Union{String, Nothing}
        let mut union_types = std::collections::BTreeSet::new();
        union_types.insert(ConcreteType::String);
        union_types.insert(ConcreteType::Nothing);
        env.set("obj.value", LatticeType::Union(union_types));

        // Condition: obj.value !== nothing
        let condition = Expr::BinaryOp {
            op: BinaryOp::NotEgal,
            left: Box::new(Expr::FieldAccess {
                object: Box::new(Expr::Var("obj".to_string(), dummy_span())),
                field: "value".to_string(),
                span: dummy_span(),
            }),
            right: Box::new(Expr::Literal(Literal::Nothing, dummy_span())),
            span: dummy_span(),
        };

        let split = split_env_by_condition(&env, &condition);

        // Then-branch: obj.value should be String (not nothing)
        assert_eq!(
            split.then_env.get("obj.value"),
            Some(&LatticeType::Concrete(ConcreteType::String))
        );

        // Else-branch: obj.value should be Nothing
        assert_eq!(
            split.else_env.get("obj.value"),
            Some(&LatticeType::Concrete(ConcreteType::Nothing))
        );
    }

    #[test]
    fn test_split_env_isa_index_access() {
        let mut env = TypeEnv::new();
        // tup[1] has type Union{Int64, String}
        let mut union_types = std::collections::BTreeSet::new();
        union_types.insert(ConcreteType::Int64);
        union_types.insert(ConcreteType::String);
        env.set("tup[1]", LatticeType::Union(union_types));

        // Condition: isa(tup[1], Int64)
        let condition = Expr::Builtin {
            name: BuiltinOp::Isa,
            args: vec![
                Expr::Index {
                    array: Box::new(Expr::Var("tup".to_string(), dummy_span())),
                    indices: vec![Expr::Literal(Literal::Int(1), dummy_span())],
                    span: dummy_span(),
                },
                Expr::Var("Int64".to_string(), dummy_span()),
            ],
            span: dummy_span(),
        };

        let split = split_env_by_condition(&env, &condition);

        // Then-branch: tup[1] should be Int64
        assert_eq!(
            split.then_env.get("tup[1]"),
            Some(&LatticeType::Concrete(ConcreteType::Int64))
        );

        // Else-branch: tup[1] should be String
        assert_eq!(
            split.else_env.get("tup[1]"),
            Some(&LatticeType::Concrete(ConcreteType::String))
        );
    }

    #[test]
    fn test_split_env_nothing_check_index_access() {
        let mut env = TypeEnv::new();
        // arr[0] has type Union{Float64, Nothing}
        let mut union_types = std::collections::BTreeSet::new();
        union_types.insert(ConcreteType::Float64);
        union_types.insert(ConcreteType::Nothing);
        env.set("arr[0]", LatticeType::Union(union_types));

        // Condition: arr[0] === nothing
        let condition = Expr::BinaryOp {
            op: BinaryOp::Egal,
            left: Box::new(Expr::Index {
                array: Box::new(Expr::Var("arr".to_string(), dummy_span())),
                indices: vec![Expr::Literal(Literal::Int(0), dummy_span())],
                span: dummy_span(),
            }),
            right: Box::new(Expr::Literal(Literal::Nothing, dummy_span())),
            span: dummy_span(),
        };

        let split = split_env_by_condition(&env, &condition);

        // Then-branch: arr[0] should be Nothing
        assert_eq!(
            split.then_env.get("arr[0]"),
            Some(&LatticeType::Concrete(ConcreteType::Nothing))
        );

        // Else-branch: arr[0] should be Float64
        assert_eq!(
            split.else_env.get("arr[0]"),
            Some(&LatticeType::Concrete(ConcreteType::Float64))
        );
    }
}
