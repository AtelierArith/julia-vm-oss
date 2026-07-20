//! Set value type.
//!
//! Split out of `container.rs` by value kind (Issue #6835).

use super::dict::DictKey;

/// Set value: unordered collection of unique elements
#[derive(Debug, Clone)]
pub struct SetValue {
    /// Storage as Vec to maintain insertion order (like Julia's Set)
    pub elements: Vec<DictKey>,
    element_type_name: Option<String>,
}

impl SetValue {
    pub fn new() -> Self {
        Self {
            elements: Vec::new(),
            element_type_name: None,
        }
    }

    pub fn with_element_type_name(element_type_name: String) -> Self {
        Self {
            elements: Vec::new(),
            element_type_name: Some(element_type_name),
        }
    }

    pub fn with_elements(elements: Vec<DictKey>) -> Self {
        let mut set = Self::new();
        for elem in elements {
            set.insert(elem);
        }
        set
    }

    pub fn insert(&mut self, elem: DictKey) -> bool {
        if self.contains(&elem) {
            false
        } else {
            self.update_element_type_name(elem.type_name());
            self.elements.push(elem);
            true
        }
    }

    fn update_element_type_name(&mut self, inserted_type: &str) {
        match self.element_type_name.as_deref() {
            None => self.element_type_name = Some(inserted_type.to_string()),
            Some("Any") => {}
            Some(existing) if existing == inserted_type => {}
            Some(_) => self.element_type_name = Some("Any".to_string()),
        }
    }

    pub fn contains(&self, elem: &DictKey) -> bool {
        self.elements.iter().any(|e| e == elem)
    }

    pub fn remove(&mut self, elem: &DictKey) -> bool {
        if let Some(pos) = self.elements.iter().position(|e| e == elem) {
            self.elements.remove(pos);
            true
        } else {
            false
        }
    }

    pub fn len(&self) -> usize {
        self.elements.len()
    }

    pub fn is_empty(&self) -> bool {
        self.elements.is_empty()
    }

    pub fn clear(&mut self) {
        self.elements.clear();
    }

    pub fn iter(&self) -> impl Iterator<Item = &DictKey> {
        self.elements.iter()
    }

    pub fn element_type_name(&self) -> &str {
        if let Some(element_type_name) = &self.element_type_name {
            return element_type_name;
        }
        let Some(first) = self.elements.first() else {
            return "Any";
        };
        let first_type = first.type_name();
        if self
            .elements
            .iter()
            .all(|element| element.type_name() == first_type)
        {
            first_type
        } else {
            "Any"
        }
    }

    /// Union: self ∪ other
    pub fn union(&self, other: &SetValue) -> SetValue {
        let mut result = self.clone();
        for elem in &other.elements {
            result.insert(elem.clone());
        }
        result
    }

    /// Intersection: self ∩ other
    pub fn intersect(&self, other: &SetValue) -> SetValue {
        let mut result = SetValue::with_element_type_name(self.element_type_name().to_string());
        for elem in &self.elements {
            if other.contains(elem) {
                result.elements.push(elem.clone());
            }
        }
        result
    }

    /// Difference: self \ other
    pub fn setdiff(&self, other: &SetValue) -> SetValue {
        let mut result = SetValue::with_element_type_name(self.element_type_name().to_string());
        for elem in &self.elements {
            if !other.contains(elem) {
                result.elements.push(elem.clone());
            }
        }
        result
    }
}

impl Default for SetValue {
    fn default() -> Self {
        Self::new()
    }
}
