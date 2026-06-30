//! Named-tuple value type.
//!
//! Split out of `container.rs` by value kind (Issue #6835).

// SAFETY: the i64→usize casts in `NamedTupleValue::get_by_index` are guarded by
// `index < 1 || index as usize > len`, ensuring a positive in-range index.
#![allow(clippy::cast_sign_loss)]

use super::super::error::VmError;
use super::Value;

/// Named tuple value: tuple with named fields
#[derive(Debug, Clone)]
pub struct NamedTupleValue {
    pub names: Vec<String>,
    pub values: Vec<Value>,
}

impl NamedTupleValue {
    pub fn new(names: Vec<String>, values: Vec<Value>) -> Result<Self, VmError> {
        if names.len() != values.len() {
            return Err(VmError::NamedTupleLengthMismatch {
                names_count: names.len(),
                values_count: values.len(),
            });
        }
        Ok(Self { names, values })
    }

    pub fn len(&self) -> usize {
        self.values.len()
    }

    pub fn is_empty(&self) -> bool {
        self.values.is_empty()
    }

    pub fn get_by_name(&self, name: &str) -> Result<&Value, VmError> {
        self.names
            .iter()
            .position(|n| n == name)
            .map(|idx| &self.values[idx])
            .ok_or_else(|| VmError::NamedTupleFieldNotFound(name.to_string()))
    }

    pub fn get_by_index(&self, index: i64) -> Result<&Value, VmError> {
        if index < 1 || index as usize > self.values.len() {
            return Err(VmError::TupleIndexOutOfBounds {
                index,
                length: self.values.len(),
            });
        }
        Ok(&self.values[(index - 1) as usize])
    }

    pub fn field_names(&self) -> &[String] {
        &self.names
    }
}
