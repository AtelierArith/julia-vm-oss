//! RangeValue - Lazy range representation for Julia's `start:step:stop` syntax.
//!
//! This module contains the `RangeValue` struct for representing lazy ranges
//! that support both integer and floating-point ranges.

// SAFETY: f64→usize casts for range len() use `.floor()` on non-negative values
// (guarded by `if step > 0 && stop >= start` or vice versa); i64→usize for
// collect() capacity uses `length()` which returns ≥ 0.
#![allow(clippy::cast_sign_loss)]

use super::super::error::VmError;
use super::ArrayValue;

/// Element-type tag retained on a [`RangeValue`] for typed integer/float ranges.
///
/// Issue #3550: `UInt8(1):UInt8(3)` must report `UnitRange{UInt8}` from
/// `typeof` and yield `UInt8` values during iteration, not the default
/// `Int64`/`Float64`.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub enum RangeElementType {
    /// No explicit tag — falls back to `Int64` for integer ranges and
    /// `Float64` for float ranges (the historical behaviour).
    #[default]
    Default,
    Int8,
    Int16,
    Int32,
    Int64,
    UInt8,
    UInt16,
    UInt32,
    UInt64,
    Float32,
    Float64,
    /// Char range, e.g. `'a':'e'`. Iteration yields `Value::Char` by
    /// converting the stored Unicode codepoint back via `char::from_u32`
    /// (Issue #4795).
    Char,
}

impl RangeElementType {
    /// The Julia type name for the element type, e.g. `"UInt8"`.
    pub fn julia_type_name(&self) -> &'static str {
        match self {
            RangeElementType::Default => "Int64",
            RangeElementType::Int8 => "Int8",
            RangeElementType::Int16 => "Int16",
            RangeElementType::Int32 => "Int32",
            RangeElementType::Int64 => "Int64",
            RangeElementType::UInt8 => "UInt8",
            RangeElementType::UInt16 => "UInt16",
            RangeElementType::UInt32 => "UInt32",
            RangeElementType::UInt64 => "UInt64",
            RangeElementType::Float32 => "Float32",
            RangeElementType::Float64 => "Float64",
            RangeElementType::Char => "Char",
        }
    }

    /// Whether the tagged element type is a floating-point type.
    pub fn is_float(&self) -> bool {
        matches!(self, RangeElementType::Float32 | RangeElementType::Float64)
    }
}

/// Lazy range value (start:step:stop)
///
/// Uses f64 to support both integer and floating-point ranges.
/// Integer ranges like 1:10 are stored as f64 but produce integer-like values.
#[derive(Debug, Clone)]
pub struct RangeValue {
    pub start: f64,
    pub step: f64,
    pub stop: f64,
    /// True if any operand was originally a Float64 (or other float type).
    /// When true, the range produces Float64 values even if all values are integer-like.
    pub is_float: bool,
    /// Original element type of the operands, retained for typed integer ranges
    /// (`UInt8(1):UInt8(3)` etc.). Used by `typeof` and iteration. Issue #3550.
    pub element_type: RangeElementType,
    /// True when the range was written with an explicit step (`a:s:b`), so it is a
    /// `StepRange` even if the step is 1. Upstream distinguishes `1:1:5`
    /// (`StepRange`) from `1:5` (`UnitRange`); without this flag a step of 1 always
    /// looked like a `UnitRange` (Issue #5667).
    pub is_step_range: bool,
}

impl RangeValue {
    fn inclusive_step_count(distance: f64, step: f64) -> i64 {
        const FLOAT_RANGE_ENDPOINT_EPSILON: f64 = 1e-10;
        ((distance / step) + FLOAT_RANGE_ENDPOINT_EPSILON).floor() as i64 + 1
    }

    /// Create a unit range (step = 1): start:stop
    pub fn unit_range(start: f64, stop: f64) -> Self {
        Self {
            start,
            step: 1.0,
            stop,
            is_float: false,
            element_type: RangeElementType::Default,
            is_step_range: false,
        }
    }

    /// Create a step range: start:step:stop (explicit step ⇒ `StepRange`).
    pub fn step_range(start: f64, step: f64, stop: f64) -> Self {
        Self {
            start,
            step,
            stop,
            is_float: false,
            element_type: RangeElementType::Default,
            is_step_range: true,
        }
    }

    /// Check if this is a unit range (`UnitRange`): step 1 AND no explicit step.
    /// `1:1:5` (explicit step 1) is a `StepRange`, not a `UnitRange` (Issue #5667).
    pub fn is_unit_range(&self) -> bool {
        self.step == 1.0 && !self.is_step_range
    }

    /// Check if this is an integer range (all values are integers)
    /// Returns true if start, step, and stop are all integer values.
    pub fn is_integer_range(&self) -> bool {
        !self.is_float
            && self.start.fract() == 0.0
            && self.step.fract() == 0.0
            && self.stop.fract() == 0.0
    }

    /// Calculate the length of the range without allocating.
    ///
    /// For integer ranges: length = floor((stop - start) / step) + 1
    /// Returns 0 for empty ranges.
    pub fn len(&self) -> usize {
        self.length().max(0) as usize
    }

    /// Calculate the length of the range as i64.
    pub fn length(&self) -> i64 {
        if self.step > 0.0 {
            if self.stop < self.start {
                0
            } else {
                Self::inclusive_step_count(self.stop - self.start, self.step)
            }
        } else if self.step < 0.0 {
            if self.stop > self.start {
                0
            } else {
                Self::inclusive_step_count(self.start - self.stop, -self.step)
            }
        } else {
            // step == 0 is invalid
            0
        }
    }

    /// Get element at 1-based index without allocating.
    pub fn get(&self, index: i64) -> Result<f64, VmError> {
        let len = self.length();
        if index < 1 || index > len {
            return Err(VmError::RangeIndexOutOfBounds { index, length: len });
        }
        Ok(self.start + (index - 1) as f64 * self.step)
    }

    /// Get the first element.
    pub fn first(&self) -> Option<f64> {
        if self.length() > 0 {
            Some(self.start)
        } else {
            None
        }
    }

    /// Get the last element.
    pub fn last(&self) -> Option<f64> {
        let len = self.length();
        if len > 0 {
            Some(self.start + (len - 1) as f64 * self.step)
        } else {
            None
        }
    }

    /// Collect the range into an ArrayValue (materializes the range).
    /// Returns Int64 array for integer ranges, Float64 array otherwise.
    pub fn collect(&self) -> ArrayValue {
        let len = self.length();
        // Issue #4795: Char ranges produce a Vector{Char} via
        // codepoint -> char conversion. Stored element type is
        // Char so iteration / indexing / show all see Chars.
        if matches!(self.element_type, RangeElementType::Char) {
            let cap = if len > 0 { len as usize } else { 0 };
            let mut data: Vec<char> = Vec::with_capacity(cap);
            for i in 0..len {
                let cp = (self.start + i as f64 * self.step) as u32;
                data.push(char::from_u32(cp).unwrap_or('\u{FFFD}'));
            }
            let shape = vec![data.len()];
            let mem = super::memory_value::MemoryValue::new(
                super::array_data::ArrayData::Char(data),
                super::array_element::ArrayElementType::Char,
                shape[0],
            );
            return ArrayValue::from_memory(mem, shape);
        }
        if self.is_integer_range() {
            // Integer range: return Int64 array
            if len <= 0 {
                return ArrayValue::memory_first_from_i64(vec![], vec![0]);
            }
            let mut data = Vec::with_capacity(len as usize);
            for i in 0..len {
                data.push(self.start as i64 + i * self.step as i64);
            }
            let len = data.len();
            ArrayValue::memory_first_from_i64(data, vec![len])
        } else {
            // Float range: return Float64 array
            if len <= 0 {
                return ArrayValue::memory_first_from_f64(vec![], vec![0]);
            }
            let mut data = Vec::with_capacity(len as usize);
            for i in 0..len {
                data.push(self.start + i as f64 * self.step);
            }
            let len = data.len();
            ArrayValue::memory_first_from_f64(data, vec![len])
        }
    }

    /// Convert the range to a Vec<f64> (materializes the range).
    pub fn to_vec(&self) -> Vec<f64> {
        let len = self.len();
        if len == 0 {
            return vec![];
        }
        let mut data = Vec::with_capacity(len);
        for i in 0..len {
            data.push(self.start + i as f64 * self.step);
        }
        data
    }

    /// Check if the range is empty.
    pub fn is_empty(&self) -> bool {
        self.len() == 0
    }

    /// Membership test for `x in r` (Issue #5728). True iff `value` is one of the
    /// range's elements. Numeric / Bool / Char values are compared by their f64
    /// representation (matching how the range stores its endpoints); other value
    /// types are never members.
    pub fn contains_value(&self, value: &super::Value) -> bool {
        use super::Value;
        let x = match value {
            Value::I64(n) => *n as f64,
            Value::I32(n) => *n as f64,
            Value::I16(n) => *n as f64,
            Value::I8(n) => *n as f64,
            Value::I128(n) => *n as f64,
            Value::U64(n) => *n as f64,
            Value::U32(n) => *n as f64,
            Value::U16(n) => *n as f64,
            Value::U8(n) => *n as f64,
            Value::U128(n) => *n as f64,
            Value::F64(v) => *v,
            Value::F32(v) => f64::from(*v),
            Value::F16(v) => v.to_f64(),
            Value::Bool(b) => {
                if *b {
                    1.0
                } else {
                    0.0
                }
            }
            Value::Char(c) => f64::from(*c as u32),
            _ => return false,
        };
        if self.step == 0.0 {
            return x == self.start;
        }
        let n = self.length();
        if n <= 0 {
            return false;
        }
        // `x` is a member iff `(x - start)/step` is a non-negative integer < length
        // (and the reconstructed value matches, guarding against float drift).
        let k = (x - self.start) / self.step;
        if k < 0.0 || k.fract() != 0.0 {
            return false;
        }
        let ki = k as i64;
        ki < n && (self.start + (ki as f64) * self.step) == x
    }

    /// Materialize a single element value (such as `first(r)` or the loop
    /// variable in iteration) at the declared element type. Issue #3550.
    pub fn typed_element(&self, val: f64) -> super::Value {
        match self.element_type {
            RangeElementType::Int8 => super::Value::I8(val as i8),
            RangeElementType::Int16 => super::Value::I16(val as i16),
            RangeElementType::Int32 => super::Value::I32(val as i32),
            RangeElementType::Int64 => super::Value::I64(val as i64),
            RangeElementType::UInt8 => super::Value::U8(val as u8),
            RangeElementType::UInt16 => super::Value::U16(val as u16),
            RangeElementType::UInt32 => super::Value::U32(val as u32),
            RangeElementType::UInt64 => super::Value::U64(val as u64),
            RangeElementType::Float32 => super::Value::F32(val as f32),
            RangeElementType::Float64 => super::Value::F64(val),
            RangeElementType::Char => {
                // Issue #4795: convert the stored codepoint back to a
                // Char. Falls back to U+FFFD (Replacement Character)
                // for invalid codepoints; matches the safe behavior of
                // `char::from_u32` so a bad range never panics.
                let cp = val as u32;
                super::Value::Char(char::from_u32(cp).unwrap_or('\u{FFFD}'))
            }
            RangeElementType::Default => {
                if self.is_integer_range() {
                    super::Value::I64(val as i64)
                } else {
                    super::Value::F64(val)
                }
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    // ── RangeValue::unit_range / step_range ───────────────────────────────────

    #[test]
    fn test_unit_range_has_step_one() {
        let r = RangeValue::unit_range(1.0, 5.0);
        assert_eq!(r.step, 1.0);
        assert!(r.is_unit_range());
    }

    #[test]
    fn test_step_range_preserves_step() {
        let r = RangeValue::step_range(0.0, 2.0, 10.0);
        assert_eq!(r.step, 2.0);
        assert!(!r.is_unit_range());
    }

    // ── RangeValue::is_integer_range ──────────────────────────────────────────

    #[test]
    fn test_integer_range_recognized() {
        let r = RangeValue::unit_range(1.0, 5.0);
        assert!(r.is_integer_range());
    }

    #[test]
    fn test_float_range_not_integer() {
        let r = RangeValue::step_range(0.0, 0.5, 2.0);
        assert!(!r.is_integer_range());
    }

    // ── RangeValue::len ───────────────────────────────────────────────────────

    #[test]
    fn test_len_of_unit_range_1_to_5() {
        // 1:5 has 5 elements: 1,2,3,4,5
        let r = RangeValue::unit_range(1.0, 5.0);
        assert_eq!(r.len(), 5);
    }

    #[test]
    fn test_len_of_single_element_range() {
        // 3:3 has 1 element
        let r = RangeValue::unit_range(3.0, 3.0);
        assert_eq!(r.len(), 1);
    }

    #[test]
    fn test_len_of_empty_range() {
        // 5:1 (step=1, stop < start) → 0 elements
        let r = RangeValue::unit_range(5.0, 1.0);
        assert_eq!(r.len(), 0);
        assert!(r.is_empty());
    }

    #[test]
    fn test_len_of_step_range_even_step() {
        // 0:2:8 → 0,2,4,6,8 = 5 elements
        let r = RangeValue::step_range(0.0, 2.0, 8.0);
        assert_eq!(r.len(), 5);
    }

    #[test]
    fn test_len_of_float_step_range_keeps_rounded_endpoint_issue_7024() {
        let r = RangeValue {
            is_float: true,
            element_type: RangeElementType::Float64,
            ..RangeValue::step_range(0.0, 0.1, 0.3)
        };
        assert_eq!(r.len(), 4);
        assert_eq!(r.get(4).unwrap(), 0.30000000000000004);
    }

    #[test]
    fn test_len_of_descending_range() {
        // 5:-1:1 → 5,4,3,2,1 = 5 elements
        let r = RangeValue::step_range(5.0, -1.0, 1.0);
        assert_eq!(r.len(), 5);
    }

    // ── RangeValue::get ───────────────────────────────────────────────────────

    #[test]
    fn test_get_first_element_is_start() {
        let r = RangeValue::unit_range(3.0, 7.0);
        let val = r.get(1).unwrap();
        assert_eq!(val, 3.0);
    }

    #[test]
    fn test_get_last_element_is_stop() {
        let r = RangeValue::unit_range(1.0, 5.0);
        let val = r.get(5).unwrap();
        assert_eq!(val, 5.0);
    }

    #[test]
    fn test_get_out_of_bounds_returns_error() {
        let r = RangeValue::unit_range(1.0, 3.0);
        assert!(
            r.get(0).is_err(),
            "Index 0 should be out of bounds (1-based)"
        );
        assert!(r.get(4).is_err(), "Index 4 should be out of bounds for 1:3");
    }

    // ── RangeValue::first / last ──────────────────────────────────────────────

    #[test]
    fn test_first_returns_start_for_nonempty_range() {
        let r = RangeValue::unit_range(2.0, 8.0);
        assert_eq!(r.first(), Some(2.0));
    }

    #[test]
    fn test_first_returns_none_for_empty_range() {
        let r = RangeValue::unit_range(5.0, 1.0);
        assert_eq!(r.first(), None);
    }

    #[test]
    fn test_last_returns_stop_for_unit_range() {
        let r = RangeValue::unit_range(1.0, 4.0);
        assert_eq!(r.last(), Some(4.0));
    }

    #[test]
    fn test_last_returns_none_for_empty_range() {
        let r = RangeValue::unit_range(10.0, 5.0);
        assert_eq!(r.last(), None);
    }

    // ── RangeValue::to_vec ────────────────────────────────────────────────────

    #[test]
    fn test_to_vec_unit_range() {
        let r = RangeValue::unit_range(1.0, 4.0);
        assert_eq!(r.to_vec(), vec![1.0, 2.0, 3.0, 4.0]);
    }

    #[test]
    fn test_to_vec_empty_range() {
        let r = RangeValue::unit_range(5.0, 1.0);
        assert_eq!(r.to_vec(), Vec::<f64>::new());
    }
}
