//! Type environment for abstract interpretation.
//!
//! This module provides the `TypeEnv` type, which tracks variable types
//! during abstract interpretation. It supports control-flow sensitive
//! type tracking through snapshots and merging.

use super::LatticeType;
#[cfg(test)]
use crate::inference_core::{CorePrimitive, CoreType};
use std::collections::{BTreeSet, HashMap};

#[derive(Clone, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
enum RefinementSegment {
    Field(String),
    Index(String),
    Raw(String),
}

#[derive(Clone, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
struct RefinementPath {
    root: String,
    segment: RefinementSegment,
}

impl RefinementPath {
    fn from_parent_path(parent: &str, path: &str) -> Self {
        let field_prefix = format!("{}.", parent);
        if let Some(field) = path.strip_prefix(&field_prefix) {
            if !field.is_empty() {
                return Self {
                    root: parent.to_string(),
                    segment: RefinementSegment::Field(field.to_string()),
                };
            }
        }

        let index_prefix = format!("{}[", parent);
        if let Some(rest) = path.strip_prefix(&index_prefix) {
            if let Some(index) = rest.strip_suffix(']') {
                return Self {
                    root: parent.to_string(),
                    segment: RefinementSegment::Index(index.to_string()),
                };
            }
        }

        Self {
            root: parent.to_string(),
            segment: RefinementSegment::Raw(path.to_string()),
        }
    }

    fn from_path(path: &str) -> Option<Self> {
        if let Some(dot) = path.find('.') {
            let root = &path[..dot];
            let field = &path[dot + 1..];
            if !root.is_empty() && !field.is_empty() {
                return Some(Self {
                    root: root.to_string(),
                    segment: RefinementSegment::Field(field.to_string()),
                });
            }
        }

        if let Some(open) = path.find('[') {
            if path.ends_with(']') {
                let root = &path[..open];
                let index = &path[open + 1..path.len() - 1];
                if !root.is_empty() && !index.is_empty() {
                    return Some(Self {
                        root: root.to_string(),
                        segment: RefinementSegment::Index(index.to_string()),
                    });
                }
            }
        }

        None
    }

    fn is_field_or_descendant_for_root(&self, root: &str, field: &str) -> bool {
        if self.root != root {
            return false;
        }
        match &self.segment {
            RefinementSegment::Field(segment) => {
                segment == field
                    || segment
                        .strip_prefix(field)
                        .is_some_and(|rest| rest.starts_with('.'))
            }
            _ => false,
        }
    }

    fn is_index_for_root(&self, root: &str) -> bool {
        self.root == root && matches!(self.segment, RefinementSegment::Index(_))
    }
}

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
/// use subset_julia_vm_types::inference_core::{CorePrimitive, CoreType};
/// use subset_julia_vm_types::runtime_types::{ConcreteType, LatticeType, TypeEnv};
///
/// let mut env = TypeEnv::new();
///
/// // Set a variable type
/// env.set("x", LatticeType::Concrete(ConcreteType::Core(CoreType::Primitive(CorePrimitive::Int64))));
///
/// // Get the type
/// let x_type = env.get("x");
/// assert_eq!(x_type, Some(&LatticeType::Concrete(ConcreteType::Core(CoreType::Primitive(CorePrimitive::Int64)))));
///
/// // Update with a new type (joins if different)
/// env.update("x", LatticeType::Concrete(ConcreteType::Core(CoreType::Primitive(CorePrimitive::Float64))));
/// ```
#[derive(Clone, Debug, PartialEq)]
pub struct TypeEnv {
    /// Map from variable name to its inferred type.
    bindings: HashMap<String, LatticeType>,
    /// Field/index refinements keyed by a structured root+segment identity.
    ///
    /// This keeps MustAlias-style facts out of ordinary variable bindings and
    /// prepares the table for SSA-versioned roots in #5601.
    refinements: HashMap<RefinementPath, LatticeType>,
    /// Parent variable -> derived field/index refinement keys.
    refinement_parents: HashMap<String, BTreeSet<RefinementPath>>,
    /// Lightweight root alias sets for local `dst = src` assignments.
    ///
    /// This is intentionally narrower than Julia's SSA `MustAlias`: it tracks
    /// only bare local roots, and rebinding a root removes just that root from
    /// the alias set. Field/index writes invalidate the matching paths through
    /// every currently known root alias.
    root_aliases: HashMap<String, BTreeSet<String>>,
    /// Local variable -> field path currently referenced by that variable.
    ///
    /// Nested field assignment lowering uses temporaries like
    /// `tmp = o.inner; tmp.val = ...`. Once `o.inner.val` is a structured
    /// refinement, mutating `tmp.val` must also invalidate `o.inner.val`.
    field_path_aliases: HashMap<String, String>,
}

impl TypeEnv {
    /// Creates a new, empty type environment.
    pub fn new() -> Self {
        Self {
            bindings: HashMap::new(),
            refinements: HashMap::new(),
            refinement_parents: HashMap::new(),
            root_aliases: HashMap::new(),
            field_path_aliases: HashMap::new(),
        }
    }

    /// Gets the type of a variable, if known.
    ///
    /// Returns `None` if the variable is not in the environment.
    ///
    /// # Example
    /// ```
    /// use subset_julia_vm_types::runtime_types::{ConcreteType, LatticeType, TypeEnv};
    /// use subset_julia_vm_types::inference_core::{CorePrimitive, CoreType};
    ///
    /// let mut env = TypeEnv::new();
    /// env.set("x", LatticeType::Concrete(ConcreteType::Core(CoreType::Primitive(CorePrimitive::Int64))));
    /// assert!(env.get("x").is_some());
    /// assert!(env.get("y").is_none());
    /// ```
    pub fn get(&self, name: &str) -> Option<&LatticeType> {
        self.bindings.get(name)
    }

    pub fn bindings(&self) -> impl Iterator<Item = (&String, &LatticeType)> {
        self.bindings.iter()
    }

    /// Sets the type of a variable.
    ///
    /// This replaces any existing type binding for the variable.
    ///
    /// # Example
    /// ```
    /// use subset_julia_vm_types::runtime_types::{ConcreteType, LatticeType, TypeEnv};
    /// use subset_julia_vm_types::inference_core::{CorePrimitive, CoreType};
    ///
    /// let mut env = TypeEnv::new();
    /// env.set("x", LatticeType::Concrete(ConcreteType::Core(CoreType::Primitive(CorePrimitive::Int64))));
    /// env.set("x", LatticeType::Concrete(ConcreteType::Core(CoreType::Primitive(CorePrimitive::Float64))));
    /// assert_eq!(env.get("x"), Some(&LatticeType::Concrete(ConcreteType::Core(CoreType::Primitive(CorePrimitive::Float64)))));
    /// ```
    pub fn set(&mut self, name: &str, ty: LatticeType) {
        self.field_path_aliases.remove(name);
        self.bindings.insert(name.to_string(), ty);
    }

    /// Records a field/index refinement and associates it with its root variable.
    pub fn set_refinement(&mut self, parent: &str, path: &str, ty: LatticeType) {
        let key = RefinementPath::from_parent_path(parent, path);
        self.refinements.insert(key.clone(), ty);
        self.refinement_parents
            .entry(parent.to_string())
            .or_default()
            .insert(key);
    }

    /// Gets a field/index refinement, if it is still valid.
    pub fn get_refinement(&self, path: &str) -> Option<&LatticeType> {
        self.refinement_lookup_key(path)
            .and_then(|key| self.refinements.get(&key))
    }

    /// Records that `dst` currently aliases `src`.
    ///
    /// The alias group is tracked so that a later mutation of any member (a
    /// field/index write, or a root rebind) can invalidate refinements held by
    /// the whole group. PartialStruct immutable-constructor field facts need
    /// no special handling here: since Issue #8739 they ride the variable's
    /// own `LatticeType::PartialStruct` binding, which `set` copies wholesale.
    ///
    /// String-keyed field/index *path* refinements (e.g. `x.value => Int64`
    /// produced by a `getfield(x, :value) !== nothing` guard) are deliberately
    /// NOT transferred to the fresh alias. Upstream Julia ties such a
    /// MustAlias narrowing to the guarded slot, not to a newly bound alias, so
    /// reads through `dst` keep the declared field union rather than the
    /// narrowed type (Issue #4844).
    pub fn alias_root(&mut self, dst: &str, src: &str) {
        if dst == src {
            return;
        }

        self.remove_root_alias(dst);

        let mut group = self.alias_roots(src);
        group.insert(src.to_string());
        group.insert(dst.to_string());

        for member in &group {
            self.root_aliases.insert(member.clone(), group.clone());
        }
    }

    /// Records that `dst` currently refers to a narrowable field path.
    pub fn alias_field_path(&mut self, dst: &str, path: &str) {
        self.field_path_aliases
            .insert(dst.to_string(), path.to_string());
    }

    /// Invalidates all refinements derived from `parent`.
    pub fn invalidate_parent(&mut self, parent: &str) {
        if let Some(paths) = self.refinement_parents.remove(parent) {
            for path in paths {
                self.refinements.remove(&path);
            }
        }
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
    /// use subset_julia_vm_types::runtime_types::{ConcreteType, LatticeType, TypeEnv};
    /// use subset_julia_vm_types::inference_core::{CorePrimitive, CoreType};
    ///
    /// let mut env = TypeEnv::new();
    /// env.set("x", LatticeType::Concrete(ConcreteType::Core(CoreType::Primitive(CorePrimitive::Int64))));
    ///
    /// // Update with Float64 - creates Union{Int64, Float64}
    /// let changed = env.update("x", LatticeType::Concrete(ConcreteType::Core(CoreType::Primitive(CorePrimitive::Float64))));
    /// assert!(changed);
    ///
    /// // Update with Int64 again - no change
    /// let changed = env.update("x", LatticeType::Concrete(ConcreteType::Core(CoreType::Primitive(CorePrimitive::Int64))));
    /// assert!(!changed);
    /// ```
    pub fn update(&mut self, name: &str, new_ty: LatticeType) -> bool {
        if let Some(existing_ty) = self.bindings.get(name) {
            let joined = existing_ty.join_limited(&new_ty, existing_ty);
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
    /// use subset_julia_vm_types::runtime_types::{ConcreteType, LatticeType, TypeEnv};
    /// use subset_julia_vm_types::inference_core::{CorePrimitive, CoreType};
    ///
    /// let mut env1 = TypeEnv::new();
    /// env1.set("x", LatticeType::Concrete(ConcreteType::Core(CoreType::Primitive(CorePrimitive::Int64))));
    /// env1.set("y", LatticeType::Concrete(ConcreteType::Core(CoreType::Primitive(CorePrimitive::String))));
    ///
    /// let mut env2 = TypeEnv::new();
    /// env2.set("x", LatticeType::Concrete(ConcreteType::Core(CoreType::Primitive(CorePrimitive::Float64))));
    /// env2.set("z", LatticeType::Concrete(ConcreteType::Core(CoreType::Primitive(CorePrimitive::Bool))));
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
        self.merge_refinements(other);
        self.merge_root_aliases(other);
        self.merge_field_path_aliases(other);
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
        let refinements_before = self.refinements.clone();
        self.merge_refinements(other);
        changed |= self.refinements != refinements_before;
        let alias_before = self.root_aliases.clone();
        self.merge_root_aliases(other);
        changed |= self.root_aliases != alias_before;
        let field_alias_before = self.field_path_aliases.clone();
        self.merge_field_path_aliases(other);
        changed |= self.field_path_aliases != field_alias_before;
        changed
    }

    fn merge_refinements(&mut self, other: &TypeEnv) {
        let mut merged = HashMap::new();
        for (key, lhs_ty) in &self.refinements {
            if let Some(rhs_ty) = other.refinements.get(key) {
                merged.insert(key.clone(), lhs_ty.join_limited(rhs_ty, lhs_ty));
            }
        }
        self.refinements = merged;
        self.rebuild_refinement_parents();
    }

    fn rebuild_refinement_parents(&mut self) {
        self.refinement_parents.clear();
        for key in self.refinements.keys() {
            self.refinement_parents
                .entry(key.root.clone())
                .or_default()
                .insert(key.clone());
        }
    }

    fn merge_root_aliases(&mut self, other: &TypeEnv) {
        let mut merged = HashMap::new();
        for (var, lhs) in &self.root_aliases {
            if let Some(rhs) = other.root_aliases.get(var) {
                let mut intersection: BTreeSet<String> = lhs.intersection(rhs).cloned().collect();
                intersection.insert(var.clone());
                if intersection.len() > 1 {
                    merged.insert(var.clone(), intersection);
                }
            }
        }
        self.root_aliases = merged;
    }

    fn merge_field_path_aliases(&mut self, other: &TypeEnv) {
        self.field_path_aliases
            .retain(|var, path| other.field_path_aliases.get(var) == Some(path));
    }

    /// Creates a snapshot of the current environment.
    ///
    /// This creates a deep copy of the environment that can be restored later.
    ///
    /// # Example
    /// ```
    /// use subset_julia_vm_types::runtime_types::{ConcreteType, LatticeType, TypeEnv};
    /// use subset_julia_vm_types::inference_core::{CorePrimitive, CoreType};
    ///
    /// let mut env = TypeEnv::new();
    /// env.set("x", LatticeType::Concrete(ConcreteType::Core(CoreType::Primitive(CorePrimitive::Int64))));
    ///
    /// let snapshot = env.snapshot();
    ///
    /// env.set("x", LatticeType::Concrete(ConcreteType::Core(CoreType::Primitive(CorePrimitive::Float64))));
    /// env.restore(snapshot);
    ///
    /// assert_eq!(env.get("x"), Some(&LatticeType::Concrete(ConcreteType::Core(CoreType::Primitive(CorePrimitive::Int64)))));
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
    /// use subset_julia_vm_types::runtime_types::{ConcreteType, LatticeType, TypeEnv};
    /// use subset_julia_vm_types::inference_core::{CorePrimitive, CoreType};
    ///
    /// let mut env = TypeEnv::new();
    /// env.set("x", LatticeType::Concrete(ConcreteType::Core(CoreType::Primitive(CorePrimitive::Int64))));
    ///
    /// let snapshot = env.snapshot();
    ///
    /// env.set("y", LatticeType::Concrete(ConcreteType::Core(CoreType::Primitive(CorePrimitive::String))));
    /// env.restore(snapshot);
    ///
    /// assert!(env.get("x").is_some());
    /// assert!(env.get("y").is_none());
    /// ```
    pub fn restore(&mut self, snapshot: TypeEnv) {
        self.bindings = snapshot.bindings;
        self.refinements = snapshot.refinements;
        self.refinement_parents = snapshot.refinement_parents;
        self.root_aliases = snapshot.root_aliases;
        self.field_path_aliases = snapshot.field_path_aliases;
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
        self.refinements.clear();
        self.refinement_parents.clear();
        self.root_aliases.clear();
        self.field_path_aliases.clear();
    }

    /// Removes a single binding by exact name.
    ///
    /// Returns the removed type, if any. Used by MustAlias-style refinement
    /// invalidation when a precise field/index path is overwritten
    /// (Issue #3504).
    pub fn remove(&mut self, name: &str) -> Option<LatticeType> {
        self.remove_refinement_path(name);
        self.remove_root_alias(name);
        self.bindings.remove(name)
    }

    /// Drops all field/index *path* refinements rooted at `var`.
    ///
    /// MustAlias-style refinements stored under composite keys like `var.f`
    /// or `var[1]` (see `conditional::extract_narrowable_path`) become stale
    /// when `var` itself is rebound, because the new value may be a different
    /// object with different fields/elements. The binding for `var` itself is
    /// **not** touched — callers update that separately via `set` (Issue #3504).
    pub fn invalidate_var_paths(&mut self, var: &str) {
        self.remove_root_alias(var);
        self.remove_field_path_alias(var);
        self.remove_field_path_aliases_under(var);
        self.invalidate_parent(var);
        let field_prefix = format!("{}.", var);
        let index_prefix = format!("{}[", var);
        self.bindings
            .retain(|k, _| !k.starts_with(&field_prefix) && !k.starts_with(&index_prefix));
    }

    /// Drops the single field-path refinement `obj.field`, if present.
    ///
    /// Used by `Stmt::FieldAssign` handling: writing one field doesn't disturb
    /// sibling fields of the same object, so we can be precise (Issue #3504).
    pub fn invalidate_field_path(&mut self, obj: &str, field: &str) {
        let mut object_paths = BTreeSet::new();
        for root in self.alias_roots(obj) {
            object_paths.insert(root.clone());
            if let Some(path) = self.field_path_aliases.get(&root) {
                object_paths.insert(path.clone());
            }
        }

        for object_path in object_paths {
            let (root, field_path) = split_field_object_path(&object_path, field);
            let to_remove: Vec<RefinementPath> = self
                .refinements
                .keys()
                .filter(|key| key.is_field_or_descendant_for_root(&root, &field_path))
                .cloned()
                .collect();
            for key in &to_remove {
                self.forget_refinement_key(key);
                self.refinements.remove(key);
            }
        }
        self.remove_field_path_aliases_for_field(obj, field);
    }

    /// Drops every index-path refinement of the form `arr[*]`.
    ///
    /// Used by `Stmt::IndexAssign` / `Stmt::DictAssign` when the index is not
    /// a constant we can match against an existing path key — without alias
    /// information we must conservatively assume any element may have been
    /// overwritten (Issue #3504).
    pub fn invalidate_index_paths(&mut self, arr: &str) {
        for root in self.alias_roots(arr) {
            let to_remove: Vec<RefinementPath> = self
                .refinements
                .keys()
                .filter(|key| key.is_index_for_root(&root))
                .cloned()
                .collect();
            for key in &to_remove {
                self.forget_refinement_key(key);
                self.refinements.remove(key);
            }

            let prefix = format!("{}[", root);
            let to_remove: Vec<String> = self
                .bindings
                .keys()
                .filter(|k| k.starts_with(&prefix))
                .cloned()
                .collect();
            for key in &to_remove {
                self.remove_refinement_path(key);
            }
            self.bindings.retain(|k, _| !k.starts_with(&prefix));
        }
    }

    fn alias_roots(&self, var: &str) -> BTreeSet<String> {
        let mut roots = self.root_aliases.get(var).cloned().unwrap_or_default();
        roots.insert(var.to_string());
        roots
    }

    fn remove_root_alias(&mut self, var: &str) {
        let Some(group) = self.root_aliases.remove(var) else {
            return;
        };
        for member in group {
            if member == var {
                continue;
            }
            let should_remove = if let Some(aliases) = self.root_aliases.get_mut(&member) {
                aliases.remove(var);
                aliases.len() <= 1
            } else {
                false
            };
            if should_remove {
                self.root_aliases.remove(&member);
            }
        }
    }

    fn remove_field_path_alias(&mut self, var: &str) {
        self.field_path_aliases.remove(var);
    }

    fn remove_field_path_aliases_under(&mut self, root: &str) {
        let field_prefix = format!("{}.", root);
        let index_prefix = format!("{}[", root);
        self.field_path_aliases
            .retain(|_, path| !path.starts_with(&field_prefix) && !path.starts_with(&index_prefix));
    }

    fn remove_field_path_aliases_for_field(&mut self, obj: &str, field: &str) {
        let (root, field_path) = split_field_object_path(obj, field);
        let nested_prefix = format!("{}.", field_path);
        self.field_path_aliases.retain(|_, path| {
            let Some(path_segment) = path.strip_prefix(&format!("{}.", root)) else {
                return true;
            };
            path_segment != field_path && !path_segment.starts_with(&nested_prefix)
        });
    }

    fn remove_refinement_path(&mut self, path: &str) -> Option<LatticeType> {
        let key = self.refinement_lookup_key(path)?;
        self.forget_refinement_key(&key);
        self.refinements.remove(&key)
    }

    fn refinement_lookup_key(&self, path: &str) -> Option<RefinementPath> {
        if let Some(key) = RefinementPath::from_path(path) {
            if self.refinements.contains_key(&key) {
                return Some(key);
            }
        }

        self.refinements
            .keys()
            .find(|key| matches!(&key.segment, RefinementSegment::Raw(raw) if raw == path))
            .cloned()
    }

    fn forget_refinement_key(&mut self, key: &RefinementPath) {
        let mut empty_parents = Vec::new();
        for (parent, paths) in &mut self.refinement_parents {
            paths.remove(key);
            if paths.is_empty() {
                empty_parents.push(parent.clone());
            }
        }
        for parent in empty_parents {
            self.refinement_parents.remove(&parent);
        }
    }
}

fn split_field_object_path(object_path: &str, field: &str) -> (String, String) {
    if let Some(dot) = object_path.find('.') {
        let root = object_path[..dot].to_string();
        let segment = format!("{}.{}", &object_path[dot + 1..], field);
        return (root, segment);
    }

    (object_path.to_string(), field.to_string())
}

impl Default for TypeEnv {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::runtime_types::ConcreteType;

    #[test]
    fn test_new_env_is_empty() {
        let env = TypeEnv::new();
        assert!(env.is_empty());
        assert_eq!(env.len(), 0);
    }

    #[test]
    fn test_set_and_get() {
        let mut env = TypeEnv::new();
        env.set(
            "x",
            LatticeType::Concrete(ConcreteType::Core(CoreType::Primitive(
                CorePrimitive::Int64,
            ))),
        );

        assert_eq!(
            env.get("x"),
            Some(&LatticeType::Concrete(ConcreteType::Core(
                CoreType::Primitive(CorePrimitive::Int64)
            )))
        );
        assert_eq!(env.get("y"), None);
    }

    #[test]
    fn test_set_overwrites() {
        let mut env = TypeEnv::new();
        env.set(
            "x",
            LatticeType::Concrete(ConcreteType::Core(CoreType::Primitive(
                CorePrimitive::Int64,
            ))),
        );
        env.set(
            "x",
            LatticeType::Concrete(ConcreteType::Core(CoreType::Primitive(
                CorePrimitive::Float64,
            ))),
        );

        assert_eq!(
            env.get("x"),
            Some(&LatticeType::Concrete(ConcreteType::Core(
                CoreType::Primitive(CorePrimitive::Float64)
            )))
        );
    }

    #[test]
    fn test_update_new_variable() {
        let mut env = TypeEnv::new();
        let changed = env.update(
            "x",
            LatticeType::Concrete(ConcreteType::Core(CoreType::Primitive(
                CorePrimitive::Int64,
            ))),
        );

        assert!(changed);
        assert_eq!(
            env.get("x"),
            Some(&LatticeType::Concrete(ConcreteType::Core(
                CoreType::Primitive(CorePrimitive::Int64)
            )))
        );
    }

    #[test]
    fn test_update_same_type_no_change() {
        let mut env = TypeEnv::new();
        env.set(
            "x",
            LatticeType::Concrete(ConcreteType::Core(CoreType::Primitive(
                CorePrimitive::Int64,
            ))),
        );

        let changed = env.update(
            "x",
            LatticeType::Concrete(ConcreteType::Core(CoreType::Primitive(
                CorePrimitive::Int64,
            ))),
        );
        assert!(!changed);
    }

    #[test]
    fn test_update_different_type_joins() {
        let mut env = TypeEnv::new();
        env.set(
            "x",
            LatticeType::Concrete(ConcreteType::Core(CoreType::Primitive(
                CorePrimitive::Int64,
            ))),
        );

        let changed = env.update(
            "x",
            LatticeType::Concrete(ConcreteType::Core(CoreType::Primitive(
                CorePrimitive::Float64,
            ))),
        );
        assert!(changed);

        // Should be a union of Int64 and Float64
        assert!(
            matches!(env.get("x"), Some(LatticeType::Union(_))),
            "Expected Union type, got {:?}",
            env.get("x")
        );
        if let Some(LatticeType::Union(types)) = env.get("x") {
            assert!(types.contains(&ConcreteType::Core(CoreType::Primitive(
                CorePrimitive::Int64
            ))));
            assert!(types.contains(&ConcreteType::Core(CoreType::Primitive(
                CorePrimitive::Float64
            ))));
        }
    }

    #[test]
    fn test_update_idempotent_after_join() {
        let mut env = TypeEnv::new();
        env.set(
            "x",
            LatticeType::Concrete(ConcreteType::Core(CoreType::Primitive(
                CorePrimitive::Int64,
            ))),
        );
        env.update(
            "x",
            LatticeType::Concrete(ConcreteType::Core(CoreType::Primitive(
                CorePrimitive::Float64,
            ))),
        );

        // Updating with Int64 again should not change (already in union)
        let changed = env.update(
            "x",
            LatticeType::Concrete(ConcreteType::Core(CoreType::Primitive(
                CorePrimitive::Int64,
            ))),
        );
        assert!(!changed);

        // Updating with Float64 again should not change
        let changed = env.update(
            "x",
            LatticeType::Concrete(ConcreteType::Core(CoreType::Primitive(
                CorePrimitive::Float64,
            ))),
        );
        assert!(!changed);
    }

    #[test]
    fn test_merge_disjoint_variables() {
        let mut env1 = TypeEnv::new();
        env1.set(
            "x",
            LatticeType::Concrete(ConcreteType::Core(CoreType::Primitive(
                CorePrimitive::Int64,
            ))),
        );

        let mut env2 = TypeEnv::new();
        env2.set(
            "y",
            LatticeType::Concrete(ConcreteType::Core(CoreType::Primitive(
                CorePrimitive::String,
            ))),
        );

        env1.merge(&env2);

        assert_eq!(
            env1.get("x"),
            Some(&LatticeType::Concrete(ConcreteType::Core(
                CoreType::Primitive(CorePrimitive::Int64)
            )))
        );
        assert_eq!(
            env1.get("y"),
            Some(&LatticeType::Concrete(ConcreteType::Core(
                CoreType::Primitive(CorePrimitive::String)
            )))
        );
    }

    #[test]
    fn test_merge_overlapping_variables() {
        let mut env1 = TypeEnv::new();
        env1.set(
            "x",
            LatticeType::Concrete(ConcreteType::Core(CoreType::Primitive(
                CorePrimitive::Int64,
            ))),
        );

        let mut env2 = TypeEnv::new();
        env2.set(
            "x",
            LatticeType::Concrete(ConcreteType::Core(CoreType::Primitive(
                CorePrimitive::Float64,
            ))),
        );

        env1.merge(&env2);

        // x should be a union
        assert!(
            matches!(env1.get("x"), Some(LatticeType::Union(_))),
            "Expected Union type, got {:?}",
            env1.get("x")
        );
        if let Some(LatticeType::Union(types)) = env1.get("x") {
            assert!(types.contains(&ConcreteType::Core(CoreType::Primitive(
                CorePrimitive::Int64
            ))));
            assert!(types.contains(&ConcreteType::Core(CoreType::Primitive(
                CorePrimitive::Float64
            ))));
        }
    }

    #[test]
    fn test_merge_mixed() {
        let mut env1 = TypeEnv::new();
        env1.set(
            "x",
            LatticeType::Concrete(ConcreteType::Core(CoreType::Primitive(
                CorePrimitive::Int64,
            ))),
        );
        env1.set(
            "y",
            LatticeType::Concrete(ConcreteType::Core(CoreType::Primitive(
                CorePrimitive::String,
            ))),
        );

        let mut env2 = TypeEnv::new();
        env2.set(
            "x",
            LatticeType::Concrete(ConcreteType::Core(CoreType::Primitive(
                CorePrimitive::Float64,
            ))),
        );
        env2.set(
            "z",
            LatticeType::Concrete(ConcreteType::Core(CoreType::Primitive(CorePrimitive::Bool))),
        );

        env1.merge(&env2);

        // x should be union
        assert!(matches!(env1.get("x"), Some(LatticeType::Union(_))));
        // y unchanged
        assert_eq!(
            env1.get("y"),
            Some(&LatticeType::Concrete(ConcreteType::Core(
                CoreType::Primitive(CorePrimitive::String)
            )))
        );
        // z added
        assert_eq!(
            env1.get("z"),
            Some(&LatticeType::Concrete(ConcreteType::Core(
                CoreType::Primitive(CorePrimitive::Bool)
            )))
        );
    }

    #[test]
    fn test_merge_drops_one_sided_refinement_issue_5858() {
        let mut narrowed = TypeEnv::new();
        narrowed.set("b", LatticeType::Top);
        narrowed.set_refinement(
            "b",
            "b.val",
            LatticeType::Concrete(ConcreteType::Core(CoreType::Primitive(
                CorePrimitive::Int64,
            ))),
        );

        let mut rebound = TypeEnv::new();
        rebound.set("b", LatticeType::Top);
        rebound.invalidate_var_paths("b");

        narrowed.merge(&rebound);

        assert!(
            narrowed.get_refinement("b.val").is_none(),
            "branch joins must not preserve a refinement absent from another incoming branch"
        );
    }

    #[test]
    fn test_merge_joins_matching_refinements() {
        let mut lhs = TypeEnv::new();
        lhs.set_refinement(
            "b",
            "b.val",
            LatticeType::Concrete(ConcreteType::Core(CoreType::Primitive(
                CorePrimitive::Int64,
            ))),
        );

        let mut rhs = TypeEnv::new();
        rhs.set_refinement(
            "b",
            "b.val",
            LatticeType::Concrete(ConcreteType::Core(CoreType::Primitive(
                CorePrimitive::Float64,
            ))),
        );

        lhs.merge(&rhs);

        assert!(matches!(
            lhs.get_refinement("b.val"),
            Some(LatticeType::Union(types))
                if types.contains(&ConcreteType::Core(CoreType::Primitive(CorePrimitive::Int64)))
                    && types.contains(&ConcreteType::Core(CoreType::Primitive(CorePrimitive::Float64)))
        ));
    }

    #[test]
    fn test_snapshot_and_restore() {
        let mut env = TypeEnv::new();
        env.set(
            "x",
            LatticeType::Concrete(ConcreteType::Core(CoreType::Primitive(
                CorePrimitive::Int64,
            ))),
        );
        env.set(
            "y",
            LatticeType::Concrete(ConcreteType::Core(CoreType::Primitive(
                CorePrimitive::String,
            ))),
        );

        let snapshot = env.snapshot();

        // Modify environment
        env.set(
            "x",
            LatticeType::Concrete(ConcreteType::Core(CoreType::Primitive(
                CorePrimitive::Float64,
            ))),
        );
        env.set(
            "z",
            LatticeType::Concrete(ConcreteType::Core(CoreType::Primitive(CorePrimitive::Bool))),
        );

        // Restore
        env.restore(snapshot);

        assert_eq!(
            env.get("x"),
            Some(&LatticeType::Concrete(ConcreteType::Core(
                CoreType::Primitive(CorePrimitive::Int64)
            )))
        );
        assert_eq!(
            env.get("y"),
            Some(&LatticeType::Concrete(ConcreteType::Core(
                CoreType::Primitive(CorePrimitive::String)
            )))
        );
        assert_eq!(env.get("z"), None);
    }

    #[test]
    fn test_snapshot_independence() {
        let mut env = TypeEnv::new();
        env.set(
            "x",
            LatticeType::Concrete(ConcreteType::Core(CoreType::Primitive(
                CorePrimitive::Int64,
            ))),
        );

        let snapshot = env.snapshot();

        // Modify original
        env.set(
            "x",
            LatticeType::Concrete(ConcreteType::Core(CoreType::Primitive(
                CorePrimitive::Float64,
            ))),
        );

        // Snapshot should be unchanged
        assert_eq!(
            snapshot.get("x"),
            Some(&LatticeType::Concrete(ConcreteType::Core(
                CoreType::Primitive(CorePrimitive::Int64)
            )))
        );
    }

    #[test]
    fn test_contains() {
        let mut env = TypeEnv::new();
        env.set(
            "x",
            LatticeType::Concrete(ConcreteType::Core(CoreType::Primitive(
                CorePrimitive::Int64,
            ))),
        );

        assert!(env.contains("x"));
        assert!(!env.contains("y"));
    }

    #[test]
    fn test_vars_iterator() {
        let mut env = TypeEnv::new();
        env.set(
            "x",
            LatticeType::Concrete(ConcreteType::Core(CoreType::Primitive(
                CorePrimitive::Int64,
            ))),
        );
        env.set(
            "y",
            LatticeType::Concrete(ConcreteType::Core(CoreType::Primitive(
                CorePrimitive::String,
            ))),
        );
        env.set(
            "z",
            LatticeType::Concrete(ConcreteType::Core(CoreType::Primitive(CorePrimitive::Bool))),
        );

        let vars: Vec<_> = env.vars().cloned().collect();
        assert_eq!(vars.len(), 3);
        assert!(vars.contains(&"x".to_string()));
        assert!(vars.contains(&"y".to_string()));
        assert!(vars.contains(&"z".to_string()));
    }

    #[test]
    fn test_clear() {
        let mut env = TypeEnv::new();
        env.set(
            "x",
            LatticeType::Concrete(ConcreteType::Core(CoreType::Primitive(
                CorePrimitive::Int64,
            ))),
        );
        env.set(
            "y",
            LatticeType::Concrete(ConcreteType::Core(CoreType::Primitive(
                CorePrimitive::String,
            ))),
        );

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

        env.set(
            "x",
            LatticeType::Concrete(ConcreteType::Core(CoreType::Primitive(
                CorePrimitive::Int64,
            ))),
        );
        assert_eq!(env.len(), 1);

        env.set(
            "y",
            LatticeType::Concrete(ConcreteType::Core(CoreType::Primitive(
                CorePrimitive::String,
            ))),
        );
        assert_eq!(env.len(), 2);

        env.set(
            "x",
            LatticeType::Concrete(ConcreteType::Core(CoreType::Primitive(
                CorePrimitive::Float64,
            ))),
        );
        assert_eq!(env.len(), 2); // Overwrite doesn't increase length
    }

    #[test]
    fn test_default() {
        let env = TypeEnv::default();
        assert!(env.is_empty());
    }

    // ====== MustAlias refinement invalidation (Issue #3504) ======

    #[test]
    fn test_invalidate_var_paths_drops_field_and_index_paths() {
        let mut env = TypeEnv::new();
        env.set(
            "obj",
            LatticeType::Concrete(ConcreteType::Core(CoreType::Primitive(
                CorePrimitive::Int64,
            ))),
        );
        env.set_refinement(
            "obj",
            "obj.value",
            LatticeType::Concrete(ConcreteType::Core(CoreType::Primitive(
                CorePrimitive::Int64,
            ))),
        );
        env.set_refinement(
            "obj",
            "obj.name",
            LatticeType::Concrete(ConcreteType::Core(CoreType::Primitive(
                CorePrimitive::String,
            ))),
        );
        env.set_refinement(
            "obj",
            "obj[1]",
            LatticeType::Concrete(ConcreteType::Core(CoreType::Primitive(
                CorePrimitive::Float64,
            ))),
        );
        env.set_refinement(
            "other",
            "other.value",
            LatticeType::Concrete(ConcreteType::Core(CoreType::Primitive(CorePrimitive::Bool))),
        );

        env.invalidate_var_paths("obj");

        // The bare binding for `obj` itself is preserved — it is the caller's
        // job to overwrite that with the new value type via `set`.
        assert!(env.contains("obj"));
        // All refinements rooted at `obj` are gone.
        assert!(env.get_refinement("obj.value").is_none());
        assert!(env.get_refinement("obj.name").is_none());
        assert!(env.get_refinement("obj[1]").is_none());
        // Sibling-namespaced paths are untouched.
        assert_eq!(
            env.get_refinement("other.value"),
            Some(&LatticeType::Concrete(ConcreteType::Core(
                CoreType::Primitive(CorePrimitive::Bool)
            )))
        );
    }

    #[test]
    fn test_invalidate_var_paths_does_not_match_prefix_lookalikes() {
        // `objx.f` shares a name prefix with `obj` but is a different variable;
        // invalidating `obj`'s paths must not touch it.
        let mut env = TypeEnv::new();
        env.set_refinement(
            "obj",
            "obj.f",
            LatticeType::Concrete(ConcreteType::Core(CoreType::Primitive(
                CorePrimitive::Int64,
            ))),
        );
        env.set_refinement(
            "objx",
            "objx.f",
            LatticeType::Concrete(ConcreteType::Core(CoreType::Primitive(
                CorePrimitive::String,
            ))),
        );
        env.set_refinement(
            "objx",
            "objx[1]",
            LatticeType::Concrete(ConcreteType::Core(CoreType::Primitive(CorePrimitive::Bool))),
        );

        env.invalidate_var_paths("obj");

        assert!(env.get_refinement("obj.f").is_none());
        assert_eq!(
            env.get_refinement("objx.f"),
            Some(&LatticeType::Concrete(ConcreteType::Core(
                CoreType::Primitive(CorePrimitive::String)
            )))
        );
        assert_eq!(
            env.get_refinement("objx[1]"),
            Some(&LatticeType::Concrete(ConcreteType::Core(
                CoreType::Primitive(CorePrimitive::Bool)
            )))
        );
    }

    #[test]
    fn test_invalidate_field_path_only_removes_named_field() {
        let mut env = TypeEnv::new();
        env.set_refinement(
            "obj",
            "obj.value",
            LatticeType::Concrete(ConcreteType::Core(CoreType::Primitive(
                CorePrimitive::Int64,
            ))),
        );
        env.set_refinement(
            "obj",
            "obj.name",
            LatticeType::Concrete(ConcreteType::Core(CoreType::Primitive(
                CorePrimitive::String,
            ))),
        );

        env.invalidate_field_path("obj", "value");

        assert!(env.get_refinement("obj.value").is_none());
        // Sibling fields of the same object survive — single-field assignment
        // can't disturb them.
        assert_eq!(
            env.get_refinement("obj.name"),
            Some(&LatticeType::Concrete(ConcreteType::Core(
                CoreType::Primitive(CorePrimitive::String)
            )))
        );
    }

    #[test]
    fn test_invalidate_field_path_removes_nested_descendants_issue_5862() {
        let mut env = TypeEnv::new();
        env.set_refinement(
            "obj",
            "obj.inner.value",
            LatticeType::Concrete(ConcreteType::Core(CoreType::Primitive(
                CorePrimitive::Int64,
            ))),
        );
        env.set_refinement(
            "obj",
            "obj.other.value",
            LatticeType::Concrete(ConcreteType::Core(CoreType::Primitive(
                CorePrimitive::String,
            ))),
        );

        env.invalidate_field_path("obj", "inner");

        assert!(env.get_refinement("obj.inner.value").is_none());
        assert_eq!(
            env.get_refinement("obj.other.value"),
            Some(&LatticeType::Concrete(ConcreteType::Core(
                CoreType::Primitive(CorePrimitive::String)
            )))
        );
    }

    #[test]
    fn test_field_path_alias_write_invalidates_nested_source_issue_5864() {
        let mut env = TypeEnv::new();
        env.set_refinement(
            "obj",
            "obj.inner.value",
            LatticeType::Concrete(ConcreteType::Core(CoreType::Primitive(
                CorePrimitive::Int64,
            ))),
        );
        env.set_refinement(
            "obj",
            "obj.other.value",
            LatticeType::Concrete(ConcreteType::Core(CoreType::Primitive(
                CorePrimitive::String,
            ))),
        );
        env.alias_field_path("tmp", "obj.inner");

        env.invalidate_field_path("tmp", "value");

        assert!(env.get_refinement("obj.inner.value").is_none());
        assert_eq!(
            env.get_refinement("obj.other.value"),
            Some(&LatticeType::Concrete(ConcreteType::Core(
                CoreType::Primitive(CorePrimitive::String)
            )))
        );
    }

    #[test]
    fn test_root_rebind_drops_field_path_alias_issue_5864() {
        let mut env = TypeEnv::new();
        env.alias_field_path("tmp", "obj.inner");
        env.invalidate_var_paths("obj");
        env.set_refinement(
            "obj",
            "obj.inner.value",
            LatticeType::Concrete(ConcreteType::Core(CoreType::Primitive(
                CorePrimitive::Int64,
            ))),
        );

        env.invalidate_field_path("tmp", "value");

        assert_eq!(
            env.get_refinement("obj.inner.value"),
            Some(&LatticeType::Concrete(ConcreteType::Core(
                CoreType::Primitive(CorePrimitive::Int64)
            )))
        );
    }

    #[test]
    fn test_invalidate_index_paths_drops_all_indexed_entries() {
        let mut env = TypeEnv::new();
        env.set_refinement(
            "arr",
            "arr[1]",
            LatticeType::Concrete(ConcreteType::Core(CoreType::Primitive(
                CorePrimitive::Int64,
            ))),
        );
        env.set_refinement(
            "arr",
            "arr[2]",
            LatticeType::Concrete(ConcreteType::Core(CoreType::Primitive(
                CorePrimitive::Float64,
            ))),
        );
        env.set_refinement(
            "arr",
            "arr.size",
            LatticeType::Concrete(ConcreteType::Core(CoreType::Primitive(
                CorePrimitive::Int64,
            ))),
        );
        env.set_refinement(
            "other",
            "other[1]",
            LatticeType::Concrete(ConcreteType::Core(CoreType::Primitive(CorePrimitive::Bool))),
        );

        env.invalidate_index_paths("arr");

        assert!(env.get_refinement("arr[1]").is_none());
        assert!(env.get_refinement("arr[2]").is_none());
        // Field-style paths on the same root are NOT touched — they live in a
        // different namespace.
        assert_eq!(
            env.get_refinement("arr.size"),
            Some(&LatticeType::Concrete(ConcreteType::Core(
                CoreType::Primitive(CorePrimitive::Int64)
            )))
        );
        assert_eq!(
            env.get_refinement("other[1]"),
            Some(&LatticeType::Concrete(ConcreteType::Core(
                CoreType::Primitive(CorePrimitive::Bool)
            )))
        );
    }

    #[test]
    fn test_remove_returns_previous_binding() {
        let mut env = TypeEnv::new();
        env.set(
            "x",
            LatticeType::Concrete(ConcreteType::Core(CoreType::Primitive(
                CorePrimitive::Int64,
            ))),
        );
        let old = env.remove("x");
        assert_eq!(
            old,
            Some(LatticeType::Concrete(ConcreteType::Core(
                CoreType::Primitive(CorePrimitive::Int64)
            )))
        );
        assert!(env.remove("x").is_none());
    }

    #[test]
    fn test_set_get_refinement_tracks_parent() {
        let mut env = TypeEnv::new();
        env.set_refinement(
            "obj",
            "obj.value",
            LatticeType::Concrete(ConcreteType::Core(CoreType::Primitive(
                CorePrimitive::Int64,
            ))),
        );

        assert_eq!(
            env.get_refinement("obj.value"),
            Some(&LatticeType::Concrete(ConcreteType::Core(
                CoreType::Primitive(CorePrimitive::Int64)
            )))
        );
        assert!(
            !env.contains("obj.value"),
            "structured refinements must not populate ordinary bindings"
        );

        env.invalidate_parent("obj");
        assert!(env.get_refinement("obj.value").is_none());
    }

    #[test]
    fn test_invalidate_parent_keeps_unrelated_refinements() {
        let mut env = TypeEnv::new();
        env.set_refinement(
            "obj",
            "obj.value",
            LatticeType::Concrete(ConcreteType::Core(CoreType::Primitive(
                CorePrimitive::Int64,
            ))),
        );
        env.set_refinement(
            "other",
            "other.value",
            LatticeType::Concrete(ConcreteType::Core(CoreType::Primitive(
                CorePrimitive::String,
            ))),
        );

        env.invalidate_parent("obj");

        assert!(env.get_refinement("obj.value").is_none());
        assert_eq!(
            env.get_refinement("other.value"),
            Some(&LatticeType::Concrete(ConcreteType::Core(
                CoreType::Primitive(CorePrimitive::String)
            )))
        );
    }

    #[test]
    fn test_alias_root_does_not_copy_field_path_refinements() {
        // Issue #4844: a fresh alias `y = x` must NOT inherit string-keyed
        // field/index path refinements from `x`. Upstream ties a MustAlias
        // field narrowing to the guarded slot, not to a newly bound alias, so
        // reads through `y` keep the declared field union.
        let mut env = TypeEnv::new();
        env.set_refinement(
            "x",
            "x.value",
            LatticeType::Concrete(ConcreteType::Core(CoreType::Primitive(
                CorePrimitive::Int64,
            ))),
        );
        env.set_refinement(
            "x",
            "x[1]",
            LatticeType::Concrete(ConcreteType::Core(CoreType::Primitive(
                CorePrimitive::String,
            ))),
        );

        env.alias_root("y", "x");

        assert!(env.get_refinement("y.value").is_none());
        assert!(env.get_refinement("y[1]").is_none());
        // The source refinements are left untouched.
        assert_eq!(
            env.get_refinement("x.value"),
            Some(&LatticeType::Concrete(ConcreteType::Core(
                CoreType::Primitive(CorePrimitive::Int64)
            )))
        );
        assert_eq!(
            env.get_refinement("x[1]"),
            Some(&LatticeType::Concrete(ConcreteType::Core(
                CoreType::Primitive(CorePrimitive::String)
            )))
        );
    }

    #[test]
    fn test_partial_struct_binding_survives_alias_and_carries_field_order() {
        // Since Issue #8739, PartialStruct immutable-constructor field facts
        // ride the variable's own `LatticeType::PartialStruct` binding (the
        // env side table is retired). A pure alias binding `y = x` copies the
        // binding wholesale via `set`, so both the by-name facts and the
        // positional field order (Issue #4269) survive; alias-group tracking
        // needs no partial-specific handling (Issue #4844).
        let mut env = TypeEnv::new();
        let fact = LatticeType::partial_struct(
            "Foo",
            7,
            vec!["x".to_string(), "y".to_string()],
            vec![
                LatticeType::Concrete(ConcreteType::Core(CoreType::Primitive(
                    CorePrimitive::Int64,
                ))),
                LatticeType::Concrete(ConcreteType::Core(CoreType::Primitive(
                    CorePrimitive::Float64,
                ))),
            ],
        );
        env.set("x", fact.clone());
        env.set("y", env.get("x").cloned().expect("x bound"));
        env.alias_root("y", "x");

        let aliased = env.get("y").expect("alias bound");
        assert_eq!(aliased, &fact);
        assert_eq!(
            aliased.partial_struct_field_by_name("x"),
            Some(&LatticeType::Concrete(ConcreteType::Core(
                CoreType::Primitive(CorePrimitive::Int64)
            )))
        );
        // Positional access lines up with the declared order.
        assert_eq!(
            aliased.partial_struct_field_by_index(2),
            Some(&LatticeType::Concrete(ConcreteType::Core(
                CoreType::Primitive(CorePrimitive::Float64)
            )))
        );
    }

    #[test]
    fn test_alias_field_write_invalidates_alias_paths() {
        // Refinements created directly on the alias (e.g. by a later guard on
        // `y`) must still be dropped when the underlying object is mutated
        // through any member of the alias group (Issue #3504).
        let mut env = TypeEnv::new();
        env.set_refinement(
            "x",
            "x.value",
            LatticeType::Concrete(ConcreteType::Core(CoreType::Primitive(
                CorePrimitive::Int64,
            ))),
        );
        env.alias_root("y", "x");
        env.set_refinement(
            "y",
            "y.value",
            LatticeType::Concrete(ConcreteType::Core(CoreType::Primitive(
                CorePrimitive::Int64,
            ))),
        );

        env.invalidate_field_path("x", "value");

        assert!(env.get_refinement("x.value").is_none());
        assert!(env.get_refinement("y.value").is_none());
    }

    #[test]
    fn test_root_rebind_drops_only_rebound_alias_root() {
        let mut env = TypeEnv::new();
        env.set_refinement(
            "x",
            "x.value",
            LatticeType::Concrete(ConcreteType::Core(CoreType::Primitive(
                CorePrimitive::Int64,
            ))),
        );
        env.alias_root("y", "x");
        env.set_refinement(
            "y",
            "y.value",
            LatticeType::Concrete(ConcreteType::Core(CoreType::Primitive(
                CorePrimitive::Int64,
            ))),
        );

        env.invalidate_var_paths("x");

        assert!(env.get_refinement("x.value").is_none());
        assert_eq!(
            env.get_refinement("y.value"),
            Some(&LatticeType::Concrete(ConcreteType::Core(
                CoreType::Primitive(CorePrimitive::Int64)
            )))
        );
    }
}
