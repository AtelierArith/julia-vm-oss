//! StructInstance - User-defined struct instances.
//!
//! This module contains the `StructInstance` struct for representing
//! user-defined struct instances, including Complex numbers.

use super::super::error::VmError;
use super::Value;

/// Well-known struct name for Complex numbers
/// Complex struct name matching Julia's Complex{Float64} type
pub const COMPLEX_STRUCT_NAME: &str = "Complex";

/// Struct instance value
#[derive(Debug, Clone)]
pub struct StructInstance {
    /// Index into the struct_defs table identifying the type
    pub type_id: usize,
    /// Name of the struct type (e.g., "Point", "Vector3D")
    pub struct_name: String,
    /// Field values in definition order
    pub values: Vec<Value>,
}

impl StructInstance {
    pub fn new(type_id: usize, values: Vec<Value>) -> Self {
        Self {
            type_id,
            struct_name: String::new(),
            values,
        }
    }

    /// Create a new struct instance with a named type
    pub fn with_name(type_id: usize, struct_name: String, values: Vec<Value>) -> Self {
        Self {
            type_id,
            struct_name,
            values,
        }
    }

    /// Create a Complex struct instance with specified type_id
    pub fn complex(type_id: usize, re: f64, im: f64) -> Self {
        Self {
            type_id,
            struct_name: COMPLEX_STRUCT_NAME.to_string(),
            values: vec![Value::F64(re), Value::F64(im)],
        }
    }

    /// Create a Complex struct instance with specified type_id
    /// Note: type_id must be looked up from struct_table at runtime
    pub fn new_complex(type_id: usize, re: f64, im: f64) -> Self {
        Self::complex(type_id, re, im)
    }

    /// Check if this is a Complex struct
    pub fn is_complex(&self) -> bool {
        // Handle both "Complex" and parametric variants like "Complex{Float64}"
        self.struct_name == COMPLEX_STRUCT_NAME
            || self
                .struct_name
                .starts_with(&format!("{}{{", COMPLEX_STRUCT_NAME))
    }

    /// Extract (re, im) from a Complex struct
    /// Returns None if not a Complex struct or fields are wrong type
    pub fn as_complex_parts(&self) -> Option<(f64, f64)> {
        if !self.is_complex() || self.values.len() != 2 {
            return None;
        }
        match (&self.values[0], &self.values[1]) {
            (Value::F64(re), Value::F64(im)) => Some((*re, *im)),
            (Value::I64(re), Value::F64(im)) => Some((*re as f64, *im)),
            (Value::F64(re), Value::I64(im)) => Some((*re, *im as f64)),
            (Value::I64(re), Value::I64(im)) => Some((*re as f64, *im as f64)),
            _ => None,
        }
    }

    pub fn get_field(&self, index: usize) -> Option<&Value> {
        self.values.get(index)
    }

    pub fn set_field(&mut self, index: usize, value: Value) -> Result<(), VmError> {
        if index < self.values.len() {
            self.values[index] = value;
            Ok(())
        } else {
            Err(VmError::FieldIndexOutOfBounds {
                index,
                field_count: self.values.len(),
            })
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    // ── constructors ──────────────────────────────────────────────────────────

    #[test]
    fn test_new_stores_type_id_and_values() {
        let s = StructInstance::new(5, vec![Value::I64(1), Value::I64(2)]);
        assert_eq!(s.type_id, 5);
        assert_eq!(s.values.len(), 2);
        assert!(s.struct_name.is_empty(), "new() should leave struct_name empty");
    }

    #[test]
    fn test_with_name_stores_name() {
        let s = StructInstance::with_name(3, "Point".to_string(), vec![Value::F64(1.0)]);
        assert_eq!(s.struct_name, "Point");
        assert_eq!(s.type_id, 3);
    }

    #[test]
    fn test_complex_constructor_sets_name_and_values() {
        let c = StructInstance::complex(0, 3.0, 4.0);
        assert_eq!(c.struct_name, COMPLEX_STRUCT_NAME);
        assert_eq!(c.values.len(), 2);
        assert!(matches!(c.values[0], Value::F64(re) if (re - 3.0).abs() < 1e-15));
        assert!(matches!(c.values[1], Value::F64(im) if (im - 4.0).abs() < 1e-15));
    }

    // ── is_complex ────────────────────────────────────────────────────────────

    #[test]
    fn test_is_complex_for_exact_name() {
        let c = StructInstance::complex(0, 1.0, 2.0);
        assert!(c.is_complex(), "\"Complex\" struct should be recognised as complex");
    }

    #[test]
    fn test_is_complex_for_parametric_name() {
        let c = StructInstance::with_name(0, "Complex{Float64}".to_string(), vec![]);
        assert!(c.is_complex(), "\"Complex{{Float64}}\" should be recognised as complex");
    }

    #[test]
    fn test_is_complex_returns_false_for_other_structs() {
        let s = StructInstance::with_name(0, "Point".to_string(), vec![]);
        assert!(!s.is_complex(), "\"Point\" struct should not be complex");
    }

    // ── as_complex_parts ──────────────────────────────────────────────────────

    #[test]
    fn test_as_complex_parts_f64_f64() {
        let c = StructInstance::complex(0, 3.0, -1.5);
        let parts = c.as_complex_parts();
        assert!(parts.is_some(), "as_complex_parts should return Some for valid Complex");
        let (re, im) = parts.unwrap();
        assert!((re - 3.0).abs() < 1e-15);
        assert!((im - (-1.5)).abs() < 1e-15);
    }

    #[test]
    fn test_as_complex_parts_i64_i64_converted_to_f64() {
        let c = StructInstance::with_name(
            0,
            "Complex".to_string(),
            vec![Value::I64(2), Value::I64(3)],
        );
        let (re, im) = c.as_complex_parts().unwrap();
        assert!((re - 2.0).abs() < 1e-15);
        assert!((im - 3.0).abs() < 1e-15);
    }

    #[test]
    fn test_as_complex_parts_returns_none_for_non_complex() {
        let s = StructInstance::with_name(0, "Point".to_string(), vec![Value::F64(1.0), Value::F64(2.0)]);
        assert!(s.as_complex_parts().is_none(), "non-Complex struct should return None");
    }

    // ── get_field / set_field ─────────────────────────────────────────────────

    #[test]
    fn test_get_field_valid_index() {
        let s = StructInstance::new(0, vec![Value::I64(42), Value::Bool(true)]);
        assert!(matches!(s.get_field(0), Some(Value::I64(42))));
        assert!(matches!(s.get_field(1), Some(Value::Bool(true))));
    }

    #[test]
    fn test_get_field_out_of_bounds_returns_none() {
        let s = StructInstance::new(0, vec![Value::I64(1)]);
        assert!(s.get_field(5).is_none(), "out-of-bounds index should return None");
    }

    #[test]
    fn test_set_field_valid_index_updates_value() {
        let mut s = StructInstance::new(0, vec![Value::I64(0)]);
        let result = s.set_field(0, Value::I64(99));
        assert!(result.is_ok(), "set_field on valid index should succeed");
        assert!(matches!(s.values[0], Value::I64(99)));
    }

    #[test]
    fn test_set_field_out_of_bounds_returns_error() {
        let mut s = StructInstance::new(0, vec![Value::I64(0)]);
        let result = s.set_field(10, Value::I64(99));
        assert!(result.is_err(), "set_field on out-of-bounds index should fail");
    }
}
