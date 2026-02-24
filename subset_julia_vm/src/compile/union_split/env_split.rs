//! Environment splitting for union-typed branches.
//!
//! This module provides functionality to split type environments based on
//! union-typed conditions, creating specialized environments for then/else
//! branches with narrowed types.

use crate::compile::abstract_interp::TypeEnv;
use crate::compile::lattice::types::{ConcreteType, LatticeType};
use crate::compile::union_split::detection::SplitCondition;

/// Result of splitting an environment based on a condition.
#[derive(Debug, Clone)]
pub struct SplitEnv {
    /// Type environment for the then-branch (condition is true)
    pub then_env: TypeEnv,
    /// Type environment for the else-branch (condition is false)
    pub else_env: TypeEnv,
}

/// Split a type environment based on a union splitting condition.
///
/// This creates two specialized environments:
/// - **Then-branch**: Type is narrowed to the target type (intersection)
/// - **Else-branch**: Type excludes the target type (subtraction)
///
/// # Arguments
///
/// * `env` - The original type environment
/// * `var` - The variable being tested
/// * `union_type` - The union type of the variable
/// * `condition` - The splitting condition
///
/// # Returns
///
/// A `SplitEnv` containing specialized environments for both branches.
///
/// # Example
///
/// ```
/// use subset_julia_vm::compile::union_split::detection::SplitCondition;
/// use subset_julia_vm::compile::union_split::split_environment;
/// use subset_julia_vm::compile::abstract_interp::TypeEnv;
/// use subset_julia_vm::compile::lattice::types::{ConcreteType, LatticeType};
/// use std::collections::BTreeSet;
///
/// // Create a type environment with x::Union{Int64, String}
/// let mut env = TypeEnv::new();
/// let mut union_members = BTreeSet::new();
/// union_members.insert(ConcreteType::Int64);
/// union_members.insert(ConcreteType::String);
/// let union_type = LatticeType::Union(union_members);
/// env.set("x", union_type.clone());
///
/// // Split on: x isa Int64
/// let condition = SplitCondition::IsaCheck {
///     target_type: ConcreteType::Int64,
/// };
/// let split = split_environment(&env, "x", &union_type, &condition);
///
/// // then_env: x narrowed to Int64
/// // else_env: x narrowed to String (union minus Int64)
/// assert!(split.then_env.get("x").is_some());
/// assert!(split.else_env.get("x").is_some());
/// ```
pub fn split_environment(
    env: &TypeEnv,
    var: &str,
    union_type: &LatticeType,
    condition: &SplitCondition,
) -> SplitEnv {
    match condition {
        SplitCondition::IsaCheck { target_type } => {
            split_isa_check(env, var, union_type, target_type)
        }
        SplitCondition::TypeofCheck { target_type } => {
            // typeof checks work the same as isa for concrete types
            split_isa_check(env, var, union_type, target_type)
        }
        SplitCondition::NothingCheck { is_equality } => {
            split_nothing_check(env, var, union_type, *is_equality)
        }
    }
}

/// Split environment for an `isa` type check.
///
/// For `x isa TargetType`:
/// - Then-branch: x is narrowed to `current_type ∩ target_type`
/// - Else-branch: x is narrowed to `current_type - target_type`
fn split_isa_check(
    env: &TypeEnv,
    var: &str,
    current_type: &LatticeType,
    target_type: &ConcreteType,
) -> SplitEnv {
    let target_lattice = LatticeType::Concrete(target_type.clone());

    // Then-branch: narrow to target type (intersection)
    let then_type = current_type.meet(&target_lattice);
    let mut then_env = env.clone();
    then_env.set(var, then_type);

    // Else-branch: exclude target type (subtraction)
    let else_type = current_type.subtract(&target_lattice);
    let mut else_env = env.clone();
    else_env.set(var, else_type);

    SplitEnv { then_env, else_env }
}

/// Split environment for a `=== nothing` check.
///
/// For `x === nothing` (is_equality = true):
/// - Then-branch: x is Nothing
/// - Else-branch: x excludes Nothing
///
/// For `x !== nothing` (is_equality = false):
/// - Then-branch: x excludes Nothing
/// - Else-branch: x is Nothing
fn split_nothing_check(
    env: &TypeEnv,
    var: &str,
    current_type: &LatticeType,
    is_equality: bool,
) -> SplitEnv {
    let nothing_type = LatticeType::Concrete(ConcreteType::Nothing);

    let (then_type, else_type) = if is_equality {
        // x === nothing:
        // - then-branch: x is Nothing
        // - else-branch: x is not Nothing
        (
            current_type.meet(&nothing_type),
            current_type.subtract(&nothing_type),
        )
    } else {
        // x !== nothing:
        // - then-branch: x is not Nothing
        // - else-branch: x is Nothing
        (
            current_type.subtract(&nothing_type),
            current_type.meet(&nothing_type),
        )
    };

    let mut then_env = env.clone();
    then_env.set(var, then_type);

    let mut else_env = env.clone();
    else_env.set(var, else_type);

    SplitEnv { then_env, else_env }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::BTreeSet;

    #[test]
    fn test_split_isa_int_from_union() {
        let mut env = TypeEnv::new();
        let mut union_types = BTreeSet::new();
        union_types.insert(ConcreteType::Int64);
        union_types.insert(ConcreteType::String);
        let union_type = LatticeType::Union(union_types);
        env.set("x", union_type.clone());

        let condition = SplitCondition::IsaCheck {
            target_type: ConcreteType::Int64,
        };

        let split = split_environment(&env, "x", &union_type, &condition);

        // Then-branch: x should be Int64
        assert_eq!(
            split.then_env.get("x"),
            Some(&LatticeType::Concrete(ConcreteType::Int64))
        );

        // Else-branch: x should be String
        assert_eq!(
            split.else_env.get("x"),
            Some(&LatticeType::Concrete(ConcreteType::String))
        );
    }

    #[test]
    fn test_split_isa_from_top() {
        let mut env = TypeEnv::new();
        env.set("x", LatticeType::Top);

        let condition = SplitCondition::IsaCheck {
            target_type: ConcreteType::Int64,
        };

        let split = split_environment(&env, "x", &LatticeType::Top, &condition);

        // Then-branch: x should be Int64
        assert_eq!(
            split.then_env.get("x"),
            Some(&LatticeType::Concrete(ConcreteType::Int64))
        );

        // Else-branch: can't subtract from Top
        assert_eq!(split.else_env.get("x"), Some(&LatticeType::Top));
    }

    #[test]
    fn test_split_nothing_equality() {
        let mut env = TypeEnv::new();
        let mut union_types = BTreeSet::new();
        union_types.insert(ConcreteType::Int64);
        union_types.insert(ConcreteType::Nothing);
        let union_type = LatticeType::Union(union_types);
        env.set("x", union_type.clone());

        let condition = SplitCondition::NothingCheck { is_equality: true };

        let split = split_environment(&env, "x", &union_type, &condition);

        // Then-branch: x === nothing, so x is Nothing
        assert_eq!(
            split.then_env.get("x"),
            Some(&LatticeType::Concrete(ConcreteType::Nothing))
        );

        // Else-branch: x !== nothing, so x is Int64
        assert_eq!(
            split.else_env.get("x"),
            Some(&LatticeType::Concrete(ConcreteType::Int64))
        );
    }

    #[test]
    fn test_split_nothing_inequality() {
        let mut env = TypeEnv::new();
        let mut union_types = BTreeSet::new();
        union_types.insert(ConcreteType::String);
        union_types.insert(ConcreteType::Nothing);
        let union_type = LatticeType::Union(union_types);
        env.set("x", union_type.clone());

        let condition = SplitCondition::NothingCheck { is_equality: false };

        let split = split_environment(&env, "x", &union_type, &condition);

        // Then-branch: x !== nothing, so x is String
        assert_eq!(
            split.then_env.get("x"),
            Some(&LatticeType::Concrete(ConcreteType::String))
        );

        // Else-branch: x === nothing, so x is Nothing
        assert_eq!(
            split.else_env.get("x"),
            Some(&LatticeType::Concrete(ConcreteType::Nothing))
        );
    }

    #[test]
    fn test_split_preserves_other_variables() {
        let mut env = TypeEnv::new();
        let mut union_types = BTreeSet::new();
        union_types.insert(ConcreteType::Int64);
        union_types.insert(ConcreteType::Float64);
        let union_type = LatticeType::Union(union_types);
        env.set("x", union_type.clone());
        env.set("y", LatticeType::Concrete(ConcreteType::String));

        let condition = SplitCondition::IsaCheck {
            target_type: ConcreteType::Int64,
        };

        let split = split_environment(&env, "x", &union_type, &condition);

        // Both branches should preserve y unchanged
        assert_eq!(
            split.then_env.get("y"),
            Some(&LatticeType::Concrete(ConcreteType::String))
        );
        assert_eq!(
            split.else_env.get("y"),
            Some(&LatticeType::Concrete(ConcreteType::String))
        );
    }

    #[test]
    fn test_split_typeof_check() {
        let mut env = TypeEnv::new();
        let mut union_types = BTreeSet::new();
        union_types.insert(ConcreteType::Bool);
        union_types.insert(ConcreteType::Int64);
        let union_type = LatticeType::Union(union_types);
        env.set("x", union_type.clone());

        let condition = SplitCondition::TypeofCheck {
            target_type: ConcreteType::Bool,
        };

        let split = split_environment(&env, "x", &union_type, &condition);

        // Then-branch: typeof(x) == Bool, so x is Bool
        assert_eq!(
            split.then_env.get("x"),
            Some(&LatticeType::Concrete(ConcreteType::Bool))
        );

        // Else-branch: typeof(x) != Bool, so x is Int64
        assert_eq!(
            split.else_env.get("x"),
            Some(&LatticeType::Concrete(ConcreteType::Int64))
        );
    }
}
