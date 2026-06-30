use serde::{Deserialize, Serialize};
use std::collections::HashMap;

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct StructHierarchyEntry {
    parent: Option<String>,
    type_params: Vec<String>,
}

impl StructHierarchyEntry {
    pub fn new(parent: Option<String>, type_params: Vec<String>) -> Self {
        Self {
            parent,
            type_params,
        }
    }

    pub fn parent(&self) -> Option<&str> {
        self.parent.as_deref()
    }

    pub fn type_params(&self) -> &[String] {
        &self.type_params
    }
}

#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct StructHierarchy {
    entries: HashMap<String, StructHierarchyEntry>,
}

impl StructHierarchy {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn from_parent_map(map: &HashMap<String, (Option<String>, Vec<String>)>) -> Self {
        let mut hierarchy = Self::new();
        for (name, (parent, type_params)) in map {
            hierarchy.insert(name, parent.clone(), type_params.clone());
        }
        hierarchy
    }

    pub fn insert(
        &mut self,
        name: impl AsRef<str>,
        parent: Option<String>,
        type_params: Vec<String>,
    ) {
        self.entries.insert(
            nominal_family_name(name.as_ref()).to_string(),
            StructHierarchyEntry::new(parent, type_params),
        );
    }

    pub fn insert_if_absent(
        &mut self,
        name: impl AsRef<str>,
        parent: Option<String>,
        type_params: Vec<String>,
    ) {
        self.entries
            .entry(nominal_family_name(name.as_ref()).to_string())
            .or_insert_with(|| StructHierarchyEntry::new(parent, type_params));
    }

    pub fn entry(&self, name: &str) -> Option<&StructHierarchyEntry> {
        self.entries.get(nominal_family_name(name))
    }

    pub fn parent_for(&self, name: &str) -> Option<Option<String>> {
        self.entry(name)
            .map(|entry| entry.parent().map(str::to_string))
    }

    pub fn parent_family_for(&self, name: &str) -> Option<Option<String>> {
        self.entry(name).map(|entry| {
            entry
                .parent()
                .map(|parent| nominal_family_name(parent).to_string())
        })
    }

    pub fn iter(&self) -> impl Iterator<Item = (&str, &StructHierarchyEntry)> {
        self.entries
            .iter()
            .map(|(name, entry)| (name.as_str(), entry))
    }

    pub fn contains_name(&self, name: &str) -> bool {
        let base = nominal_family_name(name);
        self.entries.contains_key(base)
            || self.entries.values().any(|entry| {
                entry
                    .parent()
                    .is_some_and(|parent| nominal_family_name(parent) == base)
            })
    }
}

pub fn nominal_family_name(name: &str) -> &str {
    let base = name.rfind('.').map_or(name, |idx| &name[idx + 1..]);
    base.split('{').next().unwrap_or(base)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn hierarchy_normalizes_keys_and_parent_lookup() {
        let mut hierarchy = StructHierarchy::new();
        hierarchy.insert(
            "Main.Box{Int64}",
            Some("AbstractBox{T}".to_string()),
            vec!["T".to_string()],
        );

        let entry = hierarchy.entry("Box{Float64}").unwrap();
        assert_eq!(entry.parent(), Some("AbstractBox{T}"));
        assert_eq!(entry.type_params(), ["T"]);
        assert!(hierarchy.contains_name("Main.Box{String}"));
        assert!(hierarchy.contains_name("AbstractBox{Int64}"));
        assert_eq!(
            hierarchy.parent_family_for("Box{Float64}"),
            Some(Some("AbstractBox".to_string()))
        );
    }

    #[test]
    fn insert_if_absent_preserves_first_declaration() {
        let mut hierarchy = StructHierarchy::new();
        hierarchy.insert(
            "Box",
            Some("AbstractBox{T}".to_string()),
            vec!["T".to_string()],
        );
        hierarchy.insert_if_absent("Main.Box{Int64}", Some("Any".to_string()), Vec::new());

        let entry = hierarchy.entry("Box").unwrap();
        assert_eq!(entry.parent(), Some("AbstractBox{T}"));
        assert_eq!(entry.type_params(), ["T"]);
    }
}
