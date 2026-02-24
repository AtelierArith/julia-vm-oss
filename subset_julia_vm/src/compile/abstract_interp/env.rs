//! Type environment for abstract interpretation.
//!
//! This module provides the `TypeEnv` type, which tracks variable types
//! during abstract interpretation. It supports control-flow sensitive
//! type tracking through snapshots and merging.

use crate::compile::lattice::types::LatticeType;
use std::collections::HashMap;

/// Type environment for abstract interpretation.
///
/// The `TypeEnv` tracks the types of variables during abstract interpretation,
/// supporting operations needed for control-flow sensitive type inference:
///
/// - `get`/`set`: Basic variable type lookup and assignment
/// - `update`: Join-based type update (only mutates if type changes)
/// - `merge`: Join two environments (for control flow convergence)
/// - `snapshot`/`restore`: Save and restore environment state
///
/// # Example
/// ```
/// use subset_julia_vm::compile::abstract_interp::TypeEnv;
/// use subset_julia_vm::compile::lattice::types::{ConcreteType, LatticeType};
///
/// let mut env = TypeEnv::new();
///
/// // Set a variable type
/// env.set("x", LatticeType::Concrete(ConcreteType::Int64));
///
/// // Get the type
/// let x_type = env.get("x");
/// assert_eq!(x_type, Some(&LatticeType::Concrete(ConcreteType::Int64)));
///
/// // Update with a new type (joins if different)
/// env.update("x", LatticeType::Concrete(ConcreteType::Float64));
/// ```
#[derive(Clone, Debug, PartialEq)]
pub struct TypeEnv {
    /// Map from variable name to its inferred type.
    bindings: HashMap<String, LatticeType>,
}

impl TypeEnv {
    /// Creates a new, empty type environment.
    pub fn new() -> Self {
        Self {
            bindings: HashMap::new(),
        }
    }

    /// Gets the type of a variable, if known.
    ///
    /// Returns `None` if the variable is not in the environment.
    ///
    /// # Example
    /// ```
    /// use subset_julia_vm::compile::abstract_interp::TypeEnv;
    /// use subset_julia_vm::compile::lattice::types::{ConcreteType, LatticeType};
    ///
    /// let mut env = TypeEnv::new();
    /// env.set("x", LatticeType::Concrete(ConcreteType::Int64));
    /// assert!(env.get("x").is_some());
    /// assert!(env.get("y").is_none());
    /// ```
    pub fn get(&self, name: &str) -> Option<&LatticeType> {
        self.bindings.get(name)
    }

    /// Sets the type of a variable.
    ///
    /// This replaces any existing type binding for the variable.
    ///
    /// # Example
    /// ```
    /// use subset_julia_vm::compile::abstract_interp::TypeEnv;
    /// use subset_julia_vm::compile::lattice::types::{ConcreteType, LatticeType};
    ///
    /// let mut env = TypeEnv::new();
    /// env.set("x", LatticeType::Concrete(ConcreteType::Int64));
    /// env.set("x", LatticeType::Concrete(ConcreteType::Float64));
    /// assert_eq!(env.get("x"), Some(&LatticeType::Concrete(ConcreteType::Float64)));
    /// ```
    pub fn set(&mut self, name: &str, ty: LatticeType) {
        self.bindings.insert(name.to_string(), ty);
    }

    /// Updates a variable's type using join.
    ///
    /// If the variable exists, joins the new type with the existing type.
    /// If the variable doesn't exist, sets it to the new type.
    ///
    /// Returns `true` if the type changed (environment was mutated).
    ///
    /// # Example
    /// ```
    /// use subset_julia_vm::compile::abstract_interp::TypeEnv;
    /// use subset_julia_vm::compile::lattice::types::{ConcreteType, LatticeType};
    ///
    /// let mut env = TypeEnv::new();
    /// env.set("x", LatticeType::Concrete(ConcreteType::Int64));
    ///
    /// // Update with Float64 - creates Union{Int64, Float64}
    /// let changed = env.update("x", LatticeType::Concrete(ConcreteType::Float64));
    /// assert!(changed);
    ///
    /// // Update with Int64 again - no change
    /// let changed = env.update("x", LatticeType::Concrete(ConcreteType::Int64));
    /// assert!(!changed);
    /// ```
    pub fn update(&mut self, name: &str, new_ty: LatticeType) -> bool {
        if let Some(existing_ty) = self.bindings.get(name) {
            let joined = existing_ty.join(&new_ty);
            if &joined != existing_ty {
                self.bindings.insert(name.to_string(), joined);
                true
            } else {
                false
            }
        } else {
            self.bindings.insert(name.to_string(), new_ty);
            true
        }
    }

    /// Merges another environment into this one using join.
    ///
    /// For each variable in `other`:
    /// - If the variable exists in both environments, joins the types
    /// - If the variable only exists in `other`, adds it to this environment
    ///
    /// Variables only in `self` are unchanged.
    ///
    /// # Example
    /// ```
    /// use subset_julia_vm::compile::abstract_interp::TypeEnv;
    /// use subset_julia_vm::compile::lattice::types::{ConcreteType, LatticeType};
    ///
    /// let mut env1 = TypeEnv::new();
    /// env1.set("x", LatticeType::Concrete(ConcreteType::Int64));
    /// env1.set("y", LatticeType::Concrete(ConcreteType::String));
    ///
    /// let mut env2 = TypeEnv::new();
    /// env2.set("x", LatticeType::Concrete(ConcreteType::Float64));
    /// env2.set("z", LatticeType::Concrete(ConcreteType::Bool));
    ///
    /// env1.merge(&env2);
    /// // x is now Union{Int64, Float64}
    /// // y is still String
    /// // z is now Bool
    /// ```
    pub fn merge(&mut self, other: &TypeEnv) {
        for (name, other_ty) in &other.bindings {
            self.update(name, other_ty.clone());
        }
    }

    /// Merges another environment into this one and reports if anything changed.
    ///
    /// Returns `true` if any binding was updated.
    pub fn merge_changed(&mut self, other: &TypeEnv) -> bool {
        let mut changed = false;
        for (name, other_ty) in &other.bindings {
            if self.update(name, other_ty.clone()) {
                changed = true;
            }
        }
        changed
    }

    /// Creates a snapshot of the current environment.
    ///
    /// This creates a deep copy of the environment that can be restored later.
    ///
    /// # Example
    /// ```
    /// use subset_julia_vm::compile::abstract_interp::TypeEnv;
    /// use subset_julia_vm::compile::lattice::types::{ConcreteType, LatticeType};
    ///
    /// let mut env = TypeEnv::new();
    /// env.set("x", LatticeType::Concrete(ConcreteType::Int64));
    ///
    /// let snapshot = env.snapshot();
    ///
    /// env.set("x", LatticeType::Concrete(ConcreteType::Float64));
    /// env.restore(snapshot);
    ///
    /// assert_eq!(env.get("x"), Some(&LatticeType::Concrete(ConcreteType::Int64)));
    /// ```
    pub fn snapshot(&self) -> TypeEnv {
        self.clone()
    }

    /// Restores the environment from a snapshot.
    ///
    /// This replaces the current environment with the snapshot.
    ///
    /// # Example
    /// ```
    /// use subset_julia_vm::compile::abstract_interp::TypeEnv;
    /// use subset_julia_vm::compile::lattice::types::{ConcreteType, LatticeType};
    ///
    /// let mut env = TypeEnv::new();
    /// env.set("x", LatticeType::Concrete(ConcreteType::Int64));
    ///
    /// let snapshot = env.snapshot();
    ///
    /// env.set("y", LatticeType::Concrete(ConcreteType::String));
    /// env.restore(snapshot);
    ///
    /// assert!(env.get("x").is_some());
    /// assert!(env.get("y").is_none());
    /// ```
    pub fn restore(&mut self, snapshot: TypeEnv) {
        self.bindings = snapshot.bindings;
    }

    /// Returns the number of variables in the environment.
    pub fn len(&self) -> usize {
        self.bindings.len()
    }

    /// Returns true if the environment contains no variables.
    pub fn is_empty(&self) -> bool {
        self.bindings.is_empty()
    }

    /// Returns true if the environment contains a binding for the given variable.
    pub fn contains(&self, name: &str) -> bool {
        self.bindings.contains_key(name)
    }

    /// Returns an iterator over the variable names in the environment.
    pub fn vars(&self) -> impl Iterator<Item = &String> {
        self.bindings.keys()
    }

    /// Clears all variable bindings from the environment.
    pub fn clear(&mut self) {
        self.bindings.clear();
    }
}

impl Default for TypeEnv {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::compile::lattice::types::ConcreteType;

    #[test]
    fn test_new_env_is_empty() {
        let env = TypeEnv::new();
        assert!(env.is_empty());
        assert_eq!(env.len(), 0);
    }

    #[test]
    fn test_set_and_get() {
        let mut env = TypeEnv::new();
        env.set("x", LatticeType::Concrete(ConcreteType::Int64));

        assert_eq!(
            env.get("x"),
            Some(&LatticeType::Concrete(ConcreteType::Int64))
        );
        assert_eq!(env.get("y"), None);
    }

    #[test]
    fn test_set_overwrites() {
        let mut env = TypeEnv::new();
        env.set("x", LatticeType::Concrete(ConcreteType::Int64));
        env.set("x", LatticeType::Concrete(ConcreteType::Float64));

        assert_eq!(
            env.get("x"),
            Some(&LatticeType::Concrete(ConcreteType::Float64))
        );
    }

    #[test]
    fn test_update_new_variable() {
        let mut env = TypeEnv::new();
        let changed = env.update("x", LatticeType::Concrete(ConcreteType::Int64));

        assert!(changed);
        assert_eq!(
            env.get("x"),
            Some(&LatticeType::Concrete(ConcreteType::Int64))
        );
    }

    #[test]
    fn test_update_same_type_no_change() {
        let mut env = TypeEnv::new();
        env.set("x", LatticeType::Concrete(ConcreteType::Int64));

        let changed = env.update("x", LatticeType::Concrete(ConcreteType::Int64));
        assert!(!changed);
    }

    #[test]
    fn test_update_different_type_joins() {
        let mut env = TypeEnv::new();
        env.set("x", LatticeType::Concrete(ConcreteType::Int64));

        let changed = env.update("x", LatticeType::Concrete(ConcreteType::Float64));
        assert!(changed);

        // Should be a union of Int64 and Float64
        assert!(
            matches!(env.get("x"), Some(LatticeType::Union(_))),
            "Expected Union type, got {:?}",
            env.get("x")
        );
        if let Some(LatticeType::Union(types)) = env.get("x") {
            assert!(types.contains(&ConcreteType::Int64));
            assert!(types.contains(&ConcreteType::Float64));
        }
    }

    #[test]
    fn test_update_idempotent_after_join() {
        let mut env = TypeEnv::new();
        env.set("x", LatticeType::Concrete(ConcreteType::Int64));
        env.update("x", LatticeType::Concrete(ConcreteType::Float64));

        // Updating with Int64 again should not change (already in union)
        let changed = env.update("x", LatticeType::Concrete(ConcreteType::Int64));
        assert!(!changed);

        // Updating with Float64 again should not change
        let changed = env.update("x", LatticeType::Concrete(ConcreteType::Float64));
        assert!(!changed);
    }

    #[test]
    fn test_merge_disjoint_variables() {
        let mut env1 = TypeEnv::new();
        env1.set("x", LatticeType::Concrete(ConcreteType::Int64));

        let mut env2 = TypeEnv::new();
        env2.set("y", LatticeType::Concrete(ConcreteType::String));

        env1.merge(&env2);

        assert_eq!(
            env1.get("x"),
            Some(&LatticeType::Concrete(ConcreteType::Int64))
        );
        assert_eq!(
            env1.get("y"),
            Some(&LatticeType::Concrete(ConcreteType::String))
        );
    }

    #[test]
    fn test_merge_overlapping_variables() {
        let mut env1 = TypeEnv::new();
        env1.set("x", LatticeType::Concrete(ConcreteType::Int64));

        let mut env2 = TypeEnv::new();
        env2.set("x", LatticeType::Concrete(ConcreteType::Float64));

        env1.merge(&env2);

        // x should be a union
        assert!(
            matches!(env1.get("x"), Some(LatticeType::Union(_))),
            "Expected Union type, got {:?}",
            env1.get("x")
        );
        if let Some(LatticeType::Union(types)) = env1.get("x") {
            assert!(types.contains(&ConcreteType::Int64));
            assert!(types.contains(&ConcreteType::Float64));
        }
    }

    #[test]
    fn test_merge_mixed() {
        let mut env1 = TypeEnv::new();
        env1.set("x", LatticeType::Concrete(ConcreteType::Int64));
        env1.set("y", LatticeType::Concrete(ConcreteType::String));

        let mut env2 = TypeEnv::new();
        env2.set("x", LatticeType::Concrete(ConcreteType::Float64));
        env2.set("z", LatticeType::Concrete(ConcreteType::Bool));

        env1.merge(&env2);

        // x should be union
        assert!(matches!(env1.get("x"), Some(LatticeType::Union(_))));
        // y unchanged
        assert_eq!(
            env1.get("y"),
            Some(&LatticeType::Concrete(ConcreteType::String))
        );
        // z added
        assert_eq!(
            env1.get("z"),
            Some(&LatticeType::Concrete(ConcreteType::Bool))
        );
    }

    #[test]
    fn test_snapshot_and_restore() {
        let mut env = TypeEnv::new();
        env.set("x", LatticeType::Concrete(ConcreteType::Int64));
        env.set("y", LatticeType::Concrete(ConcreteType::String));

        let snapshot = env.snapshot();

        // Modify environment
        env.set("x", LatticeType::Concrete(ConcreteType::Float64));
        env.set("z", LatticeType::Concrete(ConcreteType::Bool));

        // Restore
        env.restore(snapshot);

        assert_eq!(
            env.get("x"),
            Some(&LatticeType::Concrete(ConcreteType::Int64))
        );
        assert_eq!(
            env.get("y"),
            Some(&LatticeType::Concrete(ConcreteType::String))
        );
        assert_eq!(env.get("z"), None);
    }

    #[test]
    fn test_snapshot_independence() {
        let mut env = TypeEnv::new();
        env.set("x", LatticeType::Concrete(ConcreteType::Int64));

        let snapshot = env.snapshot();

        // Modify original
        env.set("x", LatticeType::Concrete(ConcreteType::Float64));

        // Snapshot should be unchanged
        assert_eq!(
            snapshot.get("x"),
            Some(&LatticeType::Concrete(ConcreteType::Int64))
        );
    }

    #[test]
    fn test_contains() {
        let mut env = TypeEnv::new();
        env.set("x", LatticeType::Concrete(ConcreteType::Int64));

        assert!(env.contains("x"));
        assert!(!env.contains("y"));
    }

    #[test]
    fn test_vars_iterator() {
        let mut env = TypeEnv::new();
        env.set("x", LatticeType::Concrete(ConcreteType::Int64));
        env.set("y", LatticeType::Concrete(ConcreteType::String));
        env.set("z", LatticeType::Concrete(ConcreteType::Bool));

        let vars: Vec<_> = env.vars().cloned().collect();
        assert_eq!(vars.len(), 3);
        assert!(vars.contains(&"x".to_string()));
        assert!(vars.contains(&"y".to_string()));
        assert!(vars.contains(&"z".to_string()));
    }

    #[test]
    fn test_clear() {
        let mut env = TypeEnv::new();
        env.set("x", LatticeType::Concrete(ConcreteType::Int64));
        env.set("y", LatticeType::Concrete(ConcreteType::String));

        env.clear();

        assert!(env.is_empty());
        assert_eq!(env.len(), 0);
        assert!(!env.contains("x"));
        assert!(!env.contains("y"));
    }

    #[test]
    fn test_len() {
        let mut env = TypeEnv::new();
        assert_eq!(env.len(), 0);

        env.set("x", LatticeType::Concrete(ConcreteType::Int64));
        assert_eq!(env.len(), 1);

        env.set("y", LatticeType::Concrete(ConcreteType::String));
        assert_eq!(env.len(), 2);

        env.set("x", LatticeType::Concrete(ConcreteType::Float64));
        assert_eq!(env.len(), 2); // Overwrite doesn't increase length
    }

    #[test]
    fn test_default() {
        let env = TypeEnv::default();
        assert!(env.is_empty());
    }
}
