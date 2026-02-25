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
}

impl RangeValue {
    /// Create a unit range (step = 1): start:stop
    pub fn unit_range(start: f64, stop: f64) -> Self {
        Self {
            start,
            step: 1.0,
            stop,
            is_float: false,
        }
    }

    /// Create a step range: start:step:stop
    pub fn step_range(start: f64, step: f64, stop: f64) -> Self {
        Self {
            start,
            step,
            stop,
            is_float: false,
        }
    }

    /// Check if this is a unit range (step = 1)
    pub fn is_unit_range(&self) -> bool {
        self.step == 1.0
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
        if self.step > 0.0 {
            if self.stop < self.start {
                0
            } else {
                ((self.stop - self.start) / self.step).floor() as usize + 1
            }
        } else if self.step < 0.0 {
            if self.stop > self.start {
                0
            } else {
                ((self.start - self.stop) / (-self.step)).floor() as usize + 1
            }
        } else {
            // step == 0 is invalid
            0
        }
    }

    /// Calculate the length of the range as i64.
    pub fn length(&self) -> i64 {
        if self.step > 0.0 {
            if self.stop < self.start {
                0
            } else {
                ((self.stop - self.start) / self.step).floor() as i64 + 1
            }
        } else if self.step < 0.0 {
            if self.stop > self.start {
                0
            } else {
                ((self.start - self.stop) / (-self.step)).floor() as i64 + 1
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
        if self.is_integer_range() {
            // Integer range: return Int64 array
            if len <= 0 {
                return ArrayValue::i64_vector(vec![]);
            }
            let mut data = Vec::with_capacity(len as usize);
            for i in 0..len {
                data.push(self.start as i64 + i * self.step as i64);
            }
            ArrayValue::i64_vector(data)
        } else {
            // Float range: return Float64 array
            if len <= 0 {
                return ArrayValue::vector(vec![]);
            }
            let mut data = Vec::with_capacity(len as usize);
            for i in 0..len {
                data.push(self.start + i as f64 * self.step);
            }
            ArrayValue::vector(data)
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
        assert!(r.get(0).is_err(), "Index 0 should be out of bounds (1-based)");
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
