//! MemoryValue - Flat typed memory buffer for Julia's Memory{T} primitive.
//!
//! This module contains the `MemoryValue` struct for representing Julia's
//! Memory{T} type — a flat, typed, mutable buffer without shape/dimensions.
//! Memory{T} is the low-level storage backend underlying Vector in Julia 1.11+.
//!
//! The `ArrayData` enum is reused as the storage backend, providing typed
//! storage (Vec<f64>, Vec<i64>, etc.) without the overhead of shape tracking.
#![cfg_attr(not(test), deny(clippy::unwrap_used))]

use std::cell::RefCell;
use std::rc::Rc;

use super::super::error::VmError;
use super::array_data::ArrayData;
use super::array_element::ArrayElementType;
use super::array_value::push_into_array_data;
use super::ArrayValue;
use super::Value;

/// Flat typed memory buffer (Julia's Memory{T}).
///
/// Unlike ArrayValue, MemoryValue has no shape or dimensions — it's a flat
/// buffer with a known element type and length. This matches Julia's Memory{T}
/// which is a simpler primitive than Array.
#[derive(Debug, Clone)]
pub struct MemoryValue {
    /// Type-segregated storage (reuses ArrayData for typed vectors)
    pub data: ArrayData,
    /// Element type descriptor
    pub element_type: ArrayElementType,
    /// Number of logical elements
    pub length: usize,
}

/// Shared, mutable reference to a MemoryValue (analogous to ArrayRef).
pub type MemoryRef = Rc<RefCell<MemoryValue>>;

/// Create a new MemoryRef wrapping a MemoryValue.
pub fn new_memory_ref(mem: MemoryValue) -> MemoryRef {
    Rc::new(RefCell::new(mem))
}

impl MemoryValue {
    /// Create a new MemoryValue with given data and element type.
    pub fn new(data: ArrayData, element_type: ArrayElementType, length: usize) -> Self {
        Self {
            data,
            element_type,
            length,
        }
    }

    /// Create an uninitialized Memory{T} of given length for any supported element type.
    /// Values are zero-initialized for safety (Rust doesn't have true undef).
    pub fn undef_typed(elem_type: &ArrayElementType, length: usize) -> Self {
        let data = match elem_type {
            ArrayElementType::F64 => ArrayData::F64(vec![0.0; length]),
            ArrayElementType::F32 => ArrayData::F32(vec![0.0; length]),
            ArrayElementType::I64 => ArrayData::I64(vec![0; length]),
            ArrayElementType::I32 => ArrayData::I32(vec![0; length]),
            ArrayElementType::I16 => ArrayData::I16(vec![0; length]),
            ArrayElementType::I8 => ArrayData::I8(vec![0; length]),
            ArrayElementType::U64 => ArrayData::U64(vec![0; length]),
            ArrayElementType::U32 => ArrayData::U32(vec![0; length]),
            ArrayElementType::U16 => ArrayData::U16(vec![0; length]),
            ArrayElementType::U8 => ArrayData::U8(vec![0; length]),
            ArrayElementType::Bool => ArrayData::Bool(vec![false; length]),
            ArrayElementType::String => ArrayData::String(vec![std::string::String::new(); length]),
            ArrayElementType::Char => ArrayData::Char(vec!['\0'; length]),
            ArrayElementType::Symbol => ArrayData::Any(vec![Value::Nothing; length]),
            ArrayElementType::Nothing => ArrayData::Any(vec![Value::Nothing; length]),
            _ => ArrayData::Any(vec![Value::Nothing; length]),
        };
        Self {
            data,
            element_type: elem_type.clone(),
            length,
        }
    }

    /// Create an empty, growable Memory buffer with backing `capacity` for the
    /// given element type. The logical length starts at zero and grows through
    /// [`MemoryValue::push`]. Used as the literal/comprehension build buffer
    /// after the de-variant off the native-array carrier (Issue #6807); mirrors
    /// `ArrayValue::memory_first_with_capacity`'s storage selection.
    pub fn with_capacity(element_type: ArrayElementType, capacity: usize) -> Self {
        let data = ArrayValue::capacity_data_for(&element_type, capacity);
        Self {
            data,
            element_type,
            length: 0,
        }
    }

    /// Append a value to the end, growing the buffer. Handles the special
    /// interleaved (Complex) / array-of-structs (Tuple, isbits struct) layouts
    /// via the shared [`push_into_array_data`] routine so the build buffer grows
    /// with semantics identical to `ArrayValue::push` (Issue #6807).
    pub fn push(&mut self, value: Value) -> Result<(), VmError> {
        self.length = push_into_array_data(&mut self.data, Some(&self.element_type), value)?;
        Ok(())
    }

    /// Append an `f64` value to the end (untyped F64 build buffer).
    pub fn push_f64(&mut self, value: f64) -> Result<(), VmError> {
        self.push(Value::F64(value))
    }

    /// Reserve backing capacity for at least `additional` more logical elements.
    /// Pure capacity hint: the logical length is unchanged. The raw count is
    /// scaled by the per-element storage multiplier so interleaved Complex
    /// storage (2 raw slots/element) and AoS Tuple/struct storage (one raw slot
    /// per field) reserve the right amount (mirrors `ArrayValue::reserve`).
    pub fn reserve(&mut self, additional: usize) {
        let raw_multiplier = match &self.element_type {
            ArrayElementType::ComplexF32 | ArrayElementType::ComplexF64 => 2,
            ArrayElementType::TupleOf(field_types) => field_types.len().max(1),
            ArrayElementType::StructInlineOf(_, field_count) => (*field_count).max(1),
            _ => 1,
        };
        self.data.reserve(additional.saturating_mul(raw_multiplier));
    }

    /// Whether this buffer stores heap-backed struct references (so a `Struct`
    /// value must be interned into the struct heap before being pushed).
    pub fn is_struct_ref_array(&self) -> bool {
        matches!(self.data, ArrayData::StructRefs(_))
            || matches!(self.element_type, ArrayElementType::StructOf(_))
    }

    /// Get the element type.
    pub fn element_type(&self) -> &ArrayElementType {
        &self.element_type
    }

    /// Get the number of elements.
    pub fn len(&self) -> usize {
        self.length
    }

    /// Check if the memory buffer is empty.
    pub fn is_empty(&self) -> bool {
        self.length == 0
    }

    /// Get element at 1-indexed position, returns Value.
    pub fn get(&self, index: usize) -> Result<Value, VmError> {
        if index < 1 || index > self.length {
            return Err(VmError::IndexOutOfBounds {
                indices: vec![index as i64],
                shape: vec![self.length],
            });
        }
        let linear = index - 1; // Convert to 0-indexed
        self.data
            .get_value(linear)
            .ok_or(VmError::IndexOutOfBounds {
                indices: vec![index as i64],
                shape: vec![self.length],
            })
    }

    /// Set element at 1-indexed position.
    pub fn set(&mut self, index: usize, value: Value) -> Result<(), VmError> {
        if index < 1 || index > self.length {
            return Err(VmError::IndexOutOfBounds {
                indices: vec![index as i64],
                shape: vec![self.length],
            });
        }
        let linear = index - 1; // Convert to 0-indexed
        self.data.set_value(linear, value)
    }

    /// Fill all elements with a single value.
    pub fn fill(&mut self, value: Value) -> Result<(), VmError> {
        for i in 0..self.length {
            self.data.set_value(i, value.clone())?;
        }
        Ok(())
    }

    /// Create a deep copy of this MemoryValue.
    pub fn copy(&self) -> Self {
        Self {
            data: self.data.clone(),
            element_type: self.element_type.clone(),
            length: self.length,
        }
    }

    /// Compare flat Memory contents using Julia's `isequal` scalar behavior.
    pub fn isequal_contents(&self, other: &Self) -> bool {
        if self.length != other.length {
            return false;
        }

        for i in 0..self.length {
            let left = self.data.get_value(i);
            let right = other.data.get_value(i);
            if !memory_values_isequal(left, right) {
                return false;
            }
        }
        true
    }

    /// Get the Julia type name for this Memory (e.g., "Memory{Float64}").
    pub fn julia_type_name(&self) -> String {
        format!("Memory{{{}}}", self.element_type.julia_type_name())
    }
}

/// Runtime value for Julia's `MemoryRef{T}`.
///
/// Julia stores arrays as a `MemoryRef` plus dimensions. The reference points at
/// a 1-based element offset inside its parent `Memory`; wrapping from that
/// point can address the remaining tail of the parent memory.
#[derive(Debug, Clone)]
pub struct MemoryRefValue {
    /// Parent `Memory{T}` storage.
    pub memory: MemoryRef,
    /// Zero-based element offset into `memory`.
    pub offset: usize,
}

impl MemoryRefValue {
    /// Create a `MemoryRef` at Julia's 1-based `index` into `memory`.
    pub fn new(memory: MemoryRef, index: usize) -> Result<Self, VmError> {
        let len = memory.borrow().len();
        if index < 1 || index > len + 1 {
            return Err(VmError::IndexOutOfBounds {
                indices: vec![index as i64],
                shape: vec![len],
            });
        }
        Ok(Self {
            memory,
            offset: index - 1,
        })
    }

    /// Create a `MemoryRef` at the first element.
    pub fn first(memory: MemoryRef) -> Self {
        Self { memory, offset: 0 }
    }

    /// Get the parent `Memory{T}`.
    pub fn parent(&self) -> MemoryRef {
        self.memory.clone()
    }

    /// Julia's 1-based memory index for this reference.
    pub fn memory_index(&self) -> usize {
        self.offset + 1
    }

    /// Number of elements addressable from this reference through its parent tail.
    pub fn len(&self) -> usize {
        self.memory.borrow().len().saturating_sub(self.offset)
    }

    /// Check if no elements are addressable from this reference.
    pub fn is_empty(&self) -> bool {
        self.len() == 0
    }

    /// Get the element type.
    pub fn element_type(&self) -> ArrayElementType {
        self.memory.borrow().element_type().clone()
    }

    /// Get element at 1-indexed position relative to this reference.
    pub fn get(&self, index: usize) -> Result<Value, VmError> {
        if index < 1 || index > self.len() {
            return Err(VmError::IndexOutOfBounds {
                indices: vec![index as i64],
                shape: vec![self.len()],
            });
        }
        self.memory.borrow().get(self.offset + index)
    }

    /// Set element at 1-indexed position relative to this reference.
    pub fn set(&self, index: usize, value: Value) -> Result<(), VmError> {
        if index < 1 || index > self.len() {
            return Err(VmError::IndexOutOfBounds {
                indices: vec![index as i64],
                shape: vec![self.len()],
            });
        }
        self.memory.borrow_mut().set(self.offset + index, value)
    }

    /// Get the Julia type name for this MemoryRef (e.g., "MemoryRef{Float64}").
    pub fn julia_type_name(&self) -> String {
        format!("MemoryRef{{{}}}", self.element_type().julia_type_name())
    }
}

fn memory_values_isequal(left: Option<Value>, right: Option<Value>) -> bool {
    match (left, right) {
        (Some(Value::I64(x)), Some(Value::I64(y))) => x == y,
        (Some(Value::F64(x)), Some(Value::F64(y))) => {
            x.to_bits() == y.to_bits() || (x.is_nan() && y.is_nan())
        }
        (Some(Value::I64(x)), Some(Value::F64(y))) | (Some(Value::F64(y)), Some(Value::I64(x))) => {
            (x as f64) == y
        }
        (Some(x), Some(y)) => format!("{:?}", x) == format!("{:?}", y),
        _ => false,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_memory_new_f64() {
        let mem = MemoryValue::new(
            ArrayData::F64(vec![1.0, 2.0, 3.0]),
            ArrayElementType::F64,
            3,
        );
        assert_eq!(mem.len(), 3);
        assert!(!mem.is_empty());
        assert_eq!(*mem.element_type(), ArrayElementType::F64);
    }

    #[test]
    fn test_memory_undef_typed() {
        let mem = MemoryValue::undef_typed(&ArrayElementType::I64, 5);
        assert_eq!(mem.len(), 5);
        assert_eq!(*mem.element_type(), ArrayElementType::I64);
        // All elements should be zero-initialized
        for i in 1..=5 {
            assert!(matches!(mem.get(i).unwrap(), Value::I64(0)));
        }
    }

    #[test]
    fn test_memory_get_set() {
        let mut mem = MemoryValue::undef_typed(&ArrayElementType::F64, 3);
        // Set values (1-indexed)
        mem.set(1, Value::F64(10.0)).unwrap();
        mem.set(2, Value::F64(20.0)).unwrap();
        mem.set(3, Value::F64(30.0)).unwrap();
        // Get values
        assert!(matches!(mem.get(1).unwrap(), Value::F64(v) if v == 10.0));
        assert!(matches!(mem.get(2).unwrap(), Value::F64(v) if v == 20.0));
        assert!(matches!(mem.get(3).unwrap(), Value::F64(v) if v == 30.0));
    }

    #[test]
    fn test_memory_bounds_check() {
        let mem = MemoryValue::undef_typed(&ArrayElementType::F64, 3);
        // Out of bounds: index 0
        assert!(mem.get(0).is_err());
        // Out of bounds: index 4
        assert!(mem.get(4).is_err());
    }

    #[test]
    fn test_memory_fill() {
        let mut mem = MemoryValue::undef_typed(&ArrayElementType::I64, 4);
        mem.fill(Value::I64(42)).unwrap();
        for i in 1..=4 {
            assert!(matches!(mem.get(i).unwrap(), Value::I64(42)));
        }
    }

    #[test]
    fn test_memory_copy() {
        let mut mem = MemoryValue::undef_typed(&ArrayElementType::F64, 2);
        mem.set(1, Value::F64(1.5)).unwrap();
        mem.set(2, Value::F64(2.5)).unwrap();
        let copy = mem.copy();
        assert_eq!(copy.len(), 2);
        assert!(matches!(copy.get(1).unwrap(), Value::F64(v) if v == 1.5));
        assert!(matches!(copy.get(2).unwrap(), Value::F64(v) if v == 2.5));
    }

    #[test]
    fn test_memory_isequal_contents() {
        let mut left = MemoryValue::undef_typed(&ArrayElementType::F64, 2);
        let mut right = MemoryValue::undef_typed(&ArrayElementType::F64, 2);
        left.set(1, Value::F64(f64::NAN)).unwrap();
        right.set(1, Value::F64(f64::NAN)).unwrap();
        left.set(2, Value::F64(2.5)).unwrap();
        right.set(2, Value::F64(2.5)).unwrap();

        assert!(left.isequal_contents(&right));
        right.set(2, Value::F64(3.5)).unwrap();
        assert!(!left.isequal_contents(&right));
    }

    #[test]
    fn test_memory_empty() {
        let mem = MemoryValue::undef_typed(&ArrayElementType::F64, 0);
        assert!(mem.is_empty());
        assert_eq!(mem.len(), 0);
    }

    #[test]
    fn test_memory_ref() {
        let mem = MemoryValue::undef_typed(&ArrayElementType::I64, 3);
        let mem_ref = new_memory_ref(mem);
        // Can borrow and use
        assert_eq!(mem_ref.borrow().len(), 3);
        mem_ref.borrow_mut().set(1, Value::I64(99)).unwrap();
        assert!(matches!(mem_ref.borrow().get(1).unwrap(), Value::I64(99)));
    }

    #[test]
    fn test_memory_julia_type_name() {
        let mem = MemoryValue::undef_typed(&ArrayElementType::F64, 1);
        assert_eq!(mem.julia_type_name(), "Memory{Float64}");
        let mem = MemoryValue::undef_typed(&ArrayElementType::I64, 1);
        assert_eq!(mem.julia_type_name(), "Memory{Int64}");
        let mem = MemoryValue::undef_typed(&ArrayElementType::Bool, 1);
        assert_eq!(mem.julia_type_name(), "Memory{Bool}");
    }

    #[test]
    fn test_memory_ref_get_set_uses_parent_offset() {
        let mem = new_memory_ref(MemoryValue::new(
            ArrayData::I64(vec![10, 20, 30, 40]),
            ArrayElementType::I64,
            4,
        ));
        let memref = MemoryRefValue::new(mem.clone(), 3).unwrap();

        assert_eq!(memref.memory_index(), 3);
        assert_eq!(memref.len(), 2);
        assert!(matches!(memref.get(1).unwrap(), Value::I64(30)));
        memref.set(2, Value::I64(99)).unwrap();
        assert!(matches!(mem.borrow().get(4).unwrap(), Value::I64(99)));
    }

    #[test]
    fn test_memory_ref_bounds_and_type_name() {
        let mem = new_memory_ref(MemoryValue::undef_typed(&ArrayElementType::F64, 2));
        let memref = MemoryRefValue::new(mem.clone(), 2).unwrap();

        assert_eq!(memref.julia_type_name(), "MemoryRef{Float64}");
        assert!(memref.get(0).is_err());
        assert!(memref.get(2).is_err());
        assert!(MemoryRefValue::new(mem, 4).is_err());
    }

    #[test]
    fn test_memory_set_bounds_check() {
        let mut mem = MemoryValue::undef_typed(&ArrayElementType::F64, 3);
        assert!(mem.set(0, Value::F64(1.0)).is_err());
        assert!(mem.set(4, Value::F64(1.0)).is_err());
    }

    #[test]
    fn test_memory_all_element_types() {
        // Test that undef_typed works for all basic element types
        let types = vec![
            ArrayElementType::F64,
            ArrayElementType::F32,
            ArrayElementType::I64,
            ArrayElementType::I32,
            ArrayElementType::I16,
            ArrayElementType::I8,
            ArrayElementType::U64,
            ArrayElementType::U32,
            ArrayElementType::U16,
            ArrayElementType::U8,
            ArrayElementType::Bool,
            ArrayElementType::String,
            ArrayElementType::Char,
        ];
        for et in types {
            let mem = MemoryValue::undef_typed(&et, 2);
            assert_eq!(mem.len(), 2);
            assert_eq!(*mem.element_type(), et);
        }
    }
}
