//! Tuple value type for Julia's immutable heterogeneous collections.

// SAFETY: i64→usize casts for tuple element access are guarded by bounds checks
// (`index < 1 || index as usize > elements.len()`) that ensure values are in [1, len].
#![allow(clippy::cast_sign_loss)]

use crate::vm::error::VmError;

// Forward declare Value from parent module (will be defined later in mod.rs)
// This works because Rust resolves all mod declarations before type checking
use super::Value;

/// Tuple value: heterogeneous, immutable, ordered collection
#[derive(Debug, Clone)]
pub struct TupleValue {
    pub elements: Vec<Value>,
}

impl TupleValue {
    pub fn new(elements: Vec<Value>) -> Self {
        Self { elements }
    }

    pub fn len(&self) -> usize {
        self.elements.len()
    }

    pub fn is_empty(&self) -> bool {
        self.elements.is_empty()
    }

    /// Get element at 1-based index (Julia convention)
    pub fn get(&self, index: i64) -> Result<&Value, VmError> {
        if index < 1 || index as usize > self.elements.len() {
            return Err(VmError::TupleIndexOutOfBounds {
                index,
                length: self.elements.len(),
            });
        }
        Ok(&self.elements[(index - 1) as usize])
    }

    pub fn first(&self) -> Result<&Value, VmError> {
        self.elements.first().ok_or(VmError::EmptyTuple)
    }

    pub fn last(&self) -> Result<&Value, VmError> {
        self.elements.last().ok_or(VmError::EmptyTuple)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn make_tuple(vals: Vec<i64>) -> TupleValue {
        TupleValue::new(vals.into_iter().map(Value::I64).collect())
    }

    // ── TupleValue::len / is_empty ────────────────────────────────────────────

    #[test]
    fn test_len_of_empty_tuple() {
        let t = TupleValue::new(vec![]);
        assert_eq!(t.len(), 0);
        assert!(t.is_empty());
    }

    #[test]
    fn test_len_of_three_element_tuple() {
        let t = make_tuple(vec![1, 2, 3]);
        assert_eq!(t.len(), 3);
        assert!(!t.is_empty());
    }

    // ── TupleValue::get ───────────────────────────────────────────────────────

    #[test]
    fn test_get_first_element_1_based() {
        let t = make_tuple(vec![10, 20, 30]);
        let val = t.get(1).unwrap();
        assert!(matches!(val, Value::I64(10)), "Expected I64(10), got {:?}", val);
    }

    #[test]
    fn test_get_last_element() {
        let t = make_tuple(vec![10, 20, 30]);
        let val = t.get(3).unwrap();
        assert!(matches!(val, Value::I64(30)), "Expected I64(30), got {:?}", val);
    }

    #[test]
    fn test_get_index_zero_returns_error() {
        let t = make_tuple(vec![1, 2]);
        assert!(t.get(0).is_err(), "Index 0 should be out of bounds (1-based)");
    }

    #[test]
    fn test_get_index_beyond_length_returns_error() {
        let t = make_tuple(vec![1, 2]);
        assert!(t.get(3).is_err(), "Index 3 should be out of bounds for 2-element tuple");
    }

    // ── TupleValue::first / last ──────────────────────────────────────────────

    #[test]
    fn test_first_returns_first_element() {
        let t = make_tuple(vec![42, 99]);
        let val = t.first().unwrap();
        assert!(matches!(val, Value::I64(42)), "Expected I64(42), got {:?}", val);
    }

    #[test]
    fn test_first_on_empty_tuple_returns_error() {
        let t = TupleValue::new(vec![]);
        assert!(t.first().is_err(), "first() on empty tuple should error");
    }

    #[test]
    fn test_last_returns_last_element() {
        let t = make_tuple(vec![1, 2, 7]);
        let val = t.last().unwrap();
        assert!(matches!(val, Value::I64(7)), "Expected I64(7), got {:?}", val);
    }

    #[test]
    fn test_last_on_empty_tuple_returns_error() {
        let t = TupleValue::new(vec![]);
        assert!(t.last().is_err(), "last() on empty tuple should error");
    }
}
