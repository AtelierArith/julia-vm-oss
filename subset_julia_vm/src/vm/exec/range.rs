//! Range operations for the VM.
//!
//! This module handles range creation instructions:
//! - MakeRange: Create Int64 array from integer range
//! - MakeRangeF64: Create Float64 array from float range
//! - MakeRangeLazy: Create lazy Range value (does not materialize)

// SAFETY: f64→usize cast for MakeRangeF64 capacity is from `((stop-start).abs()/step.abs()+1.0)`
// which is always non-negative (abs values). Negative results are mathematically impossible.
#![allow(clippy::cast_sign_loss)]

#![deny(clippy::unwrap_used)]
#![deny(clippy::expect_used)]

use crate::rng::RngLike;

use super::super::error::VmError;
use super::super::instr::Instr;
use super::super::stack_ops::StackOps;
use super::super::value::{new_array_ref, ArrayValue, RangeValue, Value};
use super::super::Vm;

impl<R: RngLike> Vm<R> {
    /// Execute range creation instructions.
    /// Returns `Some(())` if the instruction was handled, `None` otherwise.
    #[inline]
    pub(super) fn execute_range(&mut self, instr: &Instr) -> Result<Option<()>, VmError> {
        match instr {
            Instr::MakeRange => {
                // Create Int64 array from integer range
                let stop = self.stack.pop_i64()?;
                let step = self.stack.pop_i64()?;
                let start = self.stack.pop_i64()?;
                let capacity = if step != 0 { ((stop - start).unsigned_abs() / step.unsigned_abs() + 1) as usize } else { 0 };
                let mut data: Vec<i64> = Vec::with_capacity(capacity);
                let mut i = start;
                while (step > 0 && i <= stop) || (step < 0 && i >= stop) {
                    data.push(i);
                    i += step;
                }
                let len = data.len();
                let arr = ArrayValue::from_i64(data, vec![len]);
                self.stack.push(Value::Array(new_array_ref(arr)));
                Ok(Some(()))
            }

            Instr::MakeRangeF64 => {
                // Create Float64 array from float range
                let stop = self.pop_f64_or_i64()?;
                let step = self.pop_f64_or_i64()?;
                let start = self.pop_f64_or_i64()?;
                let mut data: Vec<f64> = Vec::with_capacity(if step.abs() > 1e-15 { ((stop - start).abs() / step.abs() + 1.0) as usize } else { 0 });
                let mut i = start;
                // Use epsilon comparison for float ranges
                while (step > 0.0 && i <= stop + 1e-10) || (step < 0.0 && i >= stop - 1e-10) {
                    data.push(i);
                    i += step;
                }
                let len = data.len();
                let arr = ArrayValue::from_f64(data, vec![len]);
                self.stack.push(Value::Array(new_array_ref(arr)));
                Ok(Some(()))
            }

            Instr::MakeRangeLazy => {
                // Create lazy Range value (does not materialize to array)
                // Use pop_f64_or_i64 to handle both integer and float parameters
                // (fixes issue #354: Range return type inference with Float64 parameters)
                let stop = self.pop_f64_or_i64()?;
                let step = self.pop_f64_or_i64()?;
                let start = self.pop_f64_or_i64()?;
                let range = RangeValue { start, step, stop };
                self.stack.push(Value::Range(range));
                Ok(Some(()))
            }

            _ => Ok(None),
        }
    }
}
