//! Merging results from union-split branches.
//!
//! This module provides functionality to merge type information and effects
//! from specialized branches back into a unified result.

use crate::compile::lattice::types::LatticeType;

/// Effects that may occur during execution.
///
/// This tracks side effects and exceptional conditions that can occur
/// in a code path, enabling more precise effect analysis.
#[derive(Debug, Clone, PartialEq)]
pub struct Effects {
    /// Whether the code path may throw an exception
    pub may_throw: bool,
    /// Whether the code path performs I/O operations
    pub has_io: bool,
    /// Whether the code path allocates memory
    pub allocates: bool,
    /// Whether the code path is guaranteed to terminate
    pub terminates: bool,
}

impl Effects {
    /// Create a new Effects with all flags set to false (pure, non-throwing).
    pub fn new() -> Self {
        Self {
            may_throw: false,
            has_io: false,
            allocates: false,
            terminates: true,
        }
    }

    /// Create Effects for code that may throw.
    pub fn throwing() -> Self {
        Self {
            may_throw: true,
            ..Self::new()
        }
    }

    /// Join two effect sets (conservative approximation).
    ///
    /// The result may have any effect that either input has.
    pub fn join(&self, other: &Effects) -> Effects {
        Effects {
            may_throw: self.may_throw || other.may_throw,
            has_io: self.has_io || other.has_io,
            allocates: self.allocates || other.allocates,
            terminates: self.terminates && other.terminates,
        }
    }
}

impl Default for Effects {
    fn default() -> Self {
        Self::new()
    }
}

/// Result of merging split branches.
#[derive(Debug, Clone)]
pub struct MergedResult {
    /// The merged return type (union of both branches)
    pub return_type: LatticeType,
    /// The merged effects (conservative approximation)
    pub effects: Effects,
}

/// Merge results from union-split branches.
///
/// This combines the return types and effects from the then-branch and
/// else-branch back into a unified result. The return type is computed
/// as the join (union) of both branch types, and effects are conservatively
/// approximated.
///
/// # Arguments
///
/// * `then_result` - Return type from the then-branch
/// * `else_result` - Return type from the else-branch
/// * `then_effects` - Effects from the then-branch
/// * `else_effects` - Effects from the else-branch
///
/// # Returns
///
/// A `MergedResult` containing the joined type and effects.
///
/// # Example
///
/// ```
/// use subset_julia_vm::compile::union_split::merge::{merge_split_results, Effects};
/// use subset_julia_vm::compile::lattice::types::{ConcreteType, LatticeType};
///
/// // Then-branch returns Int64, else-branch returns String
/// let merged = merge_split_results(
///     LatticeType::Concrete(ConcreteType::Int64),
///     LatticeType::Concrete(ConcreteType::String),
///     Effects::new(),
///     Effects::new(),
/// );
///
/// // Result type is the join (union) of both branches
/// // Effects are conservatively combined
/// assert!(!merged.effects.may_throw);
/// ```
pub fn merge_split_results(
    then_result: LatticeType,
    else_result: LatticeType,
    then_effects: Effects,
    else_effects: Effects,
) -> MergedResult {
    // Join the return types (conservative approximation)
    let return_type = then_result.join(&else_result);

    // Join the effects (if either may throw, the result may throw, etc.)
    let effects = then_effects.join(&else_effects);

    MergedResult {
        return_type,
        effects,
    }
}

/// Merge multiple specialized paths into a single result.
///
/// This is a generalization of `merge_split_results` for cases where
/// union splitting creates more than two branches (e.g., splitting a
/// three-member union).
///
/// # Arguments
///
/// * `results` - Return types from all specialized paths
/// * `effects_list` - Effects from all specialized paths
///
/// # Returns
///
/// A `MergedResult` containing the joined types and effects.
pub fn merge_multiple_paths(results: &[LatticeType], effects_list: &[Effects]) -> MergedResult {
    assert_eq!(
        results.len(),
        effects_list.len(),
        "Results and effects must have the same length"
    );

    if results.is_empty() {
        return MergedResult {
            return_type: LatticeType::Bottom,
            effects: Effects::new(),
        };
    }

    // Join all return types
    let mut return_type = results[0].clone();
    for result in &results[1..] {
        return_type = return_type.join(result);
    }

    // Join all effects
    let mut effects = effects_list[0].clone();
    for effect in &effects_list[1..] {
        effects = effects.join(effect);
    }

    MergedResult {
        return_type,
        effects,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::compile::lattice::types::ConcreteType;
    use std::collections::BTreeSet;

    #[test]
    fn test_merge_concrete_types() {
        let then_result = LatticeType::Concrete(ConcreteType::Int64);
        let else_result = LatticeType::Concrete(ConcreteType::String);
        let then_effects = Effects::new();
        let else_effects = Effects::new();

        let merged = merge_split_results(then_result, else_result, then_effects, else_effects);

        // Should create Union{Int64, String}
        assert!(
            matches!(&merged.return_type, LatticeType::Union(_)),
            "Expected Union type, got {:?}",
            merged.return_type
        );
        if let LatticeType::Union(types) = merged.return_type {
            assert_eq!(types.len(), 2);
            assert!(types.contains(&ConcreteType::Int64));
            assert!(types.contains(&ConcreteType::String));
        }

        assert!(!merged.effects.may_throw);
    }

    #[test]
    fn test_merge_with_union() {
        let then_result = LatticeType::Concrete(ConcreteType::Int64);

        let mut union_types = BTreeSet::new();
        union_types.insert(ConcreteType::Float64);
        union_types.insert(ConcreteType::Bool);
        let else_result = LatticeType::Union(union_types);

        let merged = merge_split_results(then_result, else_result, Effects::new(), Effects::new());

        // Should create Union{Int64, Float64, Bool}
        assert!(
            matches!(&merged.return_type, LatticeType::Union(_)),
            "Expected Union type, got {:?}",
            merged.return_type
        );
        if let LatticeType::Union(types) = merged.return_type {
            assert_eq!(types.len(), 3);
            assert!(types.contains(&ConcreteType::Int64));
            assert!(types.contains(&ConcreteType::Float64));
            assert!(types.contains(&ConcreteType::Bool));
        }
    }

    #[test]
    fn test_merge_effects_throwing() {
        let then_result = LatticeType::Concrete(ConcreteType::Int64);
        let else_result = LatticeType::Concrete(ConcreteType::Int64);
        let then_effects = Effects::new();
        let else_effects = Effects::throwing();

        let merged = merge_split_results(then_result, else_result, then_effects, else_effects);

        // Should propagate may_throw from else branch
        assert!(merged.effects.may_throw);
    }

    #[test]
    fn test_merge_effects_all_flags() {
        let then_result = LatticeType::Concrete(ConcreteType::Int64);
        let else_result = LatticeType::Concrete(ConcreteType::Int64);

        let then_effects = Effects {
            may_throw: true,
            has_io: false,
            allocates: true,
            terminates: true,
        };

        let else_effects = Effects {
            may_throw: false,
            has_io: true,
            allocates: false,
            terminates: true,
        };

        let merged = merge_split_results(then_result, else_result, then_effects, else_effects);

        // Should have union of all effects
        assert!(merged.effects.may_throw);
        assert!(merged.effects.has_io);
        assert!(merged.effects.allocates);
        assert!(merged.effects.terminates);
    }

    #[test]
    fn test_merge_effects_non_terminating() {
        let then_result = LatticeType::Concrete(ConcreteType::Int64);
        let else_result = LatticeType::Concrete(ConcreteType::Int64);

        let then_effects = Effects {
            may_throw: false,
            has_io: false,
            allocates: false,
            terminates: true,
        };

        let else_effects = Effects {
            may_throw: false,
            has_io: false,
            allocates: false,
            terminates: false, // May loop forever
        };

        let merged = merge_split_results(then_result, else_result, then_effects, else_effects);

        // If any path may not terminate, result may not terminate
        assert!(!merged.effects.terminates);
    }

    #[test]
    fn test_merge_multiple_paths_empty() {
        let merged = merge_multiple_paths(&[], &[]);

        assert_eq!(merged.return_type, LatticeType::Bottom);
        assert!(!merged.effects.may_throw);
    }

    #[test]
    fn test_merge_multiple_paths_three() {
        let results = vec![
            LatticeType::Concrete(ConcreteType::Int64),
            LatticeType::Concrete(ConcreteType::Float64),
            LatticeType::Concrete(ConcreteType::String),
        ];

        let effects_list = vec![Effects::new(), Effects::throwing(), Effects::new()];

        let merged = merge_multiple_paths(&results, &effects_list);

        // Should create Union{Int64, Float64, String}
        assert!(
            matches!(&merged.return_type, LatticeType::Union(_)),
            "Expected Union type, got {:?}",
            merged.return_type
        );
        if let LatticeType::Union(types) = merged.return_type {
            assert_eq!(types.len(), 3);
            assert!(types.contains(&ConcreteType::Int64));
            assert!(types.contains(&ConcreteType::Float64));
            assert!(types.contains(&ConcreteType::String));
        }

        // Should propagate throwing effect
        assert!(merged.effects.may_throw);
    }

    #[test]
    fn test_effects_join() {
        let e1 = Effects {
            may_throw: true,
            has_io: false,
            allocates: true,
            terminates: true,
        };

        let e2 = Effects {
            may_throw: false,
            has_io: true,
            allocates: false,
            terminates: false,
        };

        let joined = e1.join(&e2);

        assert!(joined.may_throw);
        assert!(joined.has_io);
        assert!(joined.allocates);
        assert!(!joined.terminates);
    }
}
