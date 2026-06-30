//! Facade for the shared CoreType subtype solver.
//!
//! Issue #5915 tracks moving the scattered compile-time/runtime subtype and
//! type-match entry points behind one engine. This is intentionally thin for
//! now: callers still own their legacy fallbacks, but the structured CoreType
//! query is centralized here so call sites can migrate without choosing a new
//! representation each time.

use crate::types::StructHierarchy;

use super::CoreType;

pub struct CoreSubtypeEngine<'a> {
    hierarchy: Option<&'a StructHierarchy>,
}

impl<'a> CoreSubtypeEngine<'a> {
    pub fn new() -> Self {
        Self { hierarchy: None }
    }

    pub fn with_hierarchy(hierarchy: &'a StructHierarchy) -> Self {
        Self {
            hierarchy: Some(hierarchy),
        }
    }

    pub fn is_subtype(&self, left: &CoreType, right: &CoreType) -> bool {
        match self.hierarchy {
            Some(hierarchy) => left.is_subtype_of_with_hierarchy(right, hierarchy),
            None => left.is_subtype_of(right),
        }
    }

    pub fn is_subtype_by_name(&self, left: &str, right: &str) -> bool {
        self.is_subtype(
            &CoreType::from_julia_name(left),
            &CoreType::from_julia_name(right),
        )
    }
}

impl Default for CoreSubtypeEngine<'_> {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn builtin_name_queries_use_coretype_solver() {
        let engine = CoreSubtypeEngine::new();

        assert!(engine.is_subtype_by_name("Int64", "Integer"));
        assert!(engine.is_subtype_by_name("Vector{Int64}", "AbstractVector"));
        assert!(!engine.is_subtype_by_name("String", "Integer"));
    }

    #[test]
    fn hierarchy_queries_use_supplied_struct_hierarchy() {
        let mut hierarchy = StructHierarchy::new();
        hierarchy.insert("Animal", Some("Any".to_string()), Vec::new());
        hierarchy.insert("Dog", Some("Animal".to_string()), Vec::new());

        let engine = CoreSubtypeEngine::with_hierarchy(&hierarchy);

        assert!(engine.is_subtype_by_name("Dog", "Animal"));
        assert!(!CoreSubtypeEngine::new().is_subtype_by_name("Dog", "Animal"));
    }

    /// Issue #5915 wave 3: a bare user name (struct or `abstract type`) whose
    /// declared chain reaches a BUILT-IN abstract is decided by the engine — the
    /// `(Named, Abstract)` arm walks the registered hierarchy into the numeric
    /// lattice, so the runtime no longer needs a separate `type_ancestors`
    /// fallback. Verified against upstream `julia` 1.12.
    #[test]
    fn named_user_type_reaches_builtin_abstract_through_hierarchy() {
        let mut hierarchy = StructHierarchy::new();
        hierarchy.insert("Money", Some("Real".to_string()), Vec::new());
        hierarchy.insert("Currency", Some("Number".to_string()), Vec::new());

        let engine = CoreSubtypeEngine::with_hierarchy(&hierarchy);
        assert!(engine.is_subtype_by_name("Money", "Real"));
        assert!(engine.is_subtype_by_name("Money", "Number"));
        assert!(!engine.is_subtype_by_name("Money", "AbstractFloat"));
        assert!(engine.is_subtype_by_name("Currency", "Number"));
        assert!(!engine.is_subtype_by_name("Currency", "Real"));
        // Unknown name is authoritatively not a subtype of a built-in abstract.
        assert!(!engine.is_subtype_by_name("Mystery", "Real"));
        // No hierarchy → nothing known.
        assert!(!CoreSubtypeEngine::new().is_subtype_by_name("Money", "Real"));
    }
}
