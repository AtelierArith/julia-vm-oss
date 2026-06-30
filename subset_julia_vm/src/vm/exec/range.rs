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
use super::super::value::{ArrayValue, RangeElementType, RangeValue, Value};
use super::super::Vm;
use super::DispatchAction;

impl<R: RngLike> Vm<R> {
    /// Execute range creation instructions.
    /// Returns `Some(())` if the instruction was handled, `None` otherwise.
    #[inline]
    pub(super) fn execute_range(&mut self, instr: &Instr) -> Result<DispatchAction, VmError> {
        match instr {
            Instr::MakeRange => {
                // Create Int64 array from integer range
                let stop = self.stack.pop_i64()?;
                let step = self.stack.pop_i64()?;
                let start = self.stack.pop_i64()?;
                let capacity = if step != 0 {
                    ((stop - start).unsigned_abs() / step.unsigned_abs() + 1) as usize
                } else {
                    0
                };
                let mut data: Vec<i64> = Vec::with_capacity(capacity);
                let mut i = start;
                while (step > 0 && i <= stop) || (step < 0 && i >= stop) {
                    data.push(i);
                    i += step;
                }
                let len = data.len();
                let arr = ArrayValue::memory_first_from_i64(data, vec![len]);
                self.push_array_value_as_wrapper(arr)?;
                Ok(DispatchAction::Continue)
            }

            Instr::MakeRangeF64 => {
                // Create Float64 array from float range
                let stop = self.pop_f64_or_i64()?;
                let step = self.pop_f64_or_i64()?;
                let start = self.pop_f64_or_i64()?;
                let mut data: Vec<f64> = Vec::with_capacity(if step.abs() > 1e-15 {
                    ((stop - start).abs() / step.abs() + 1.0) as usize
                } else {
                    0
                });
                let mut i = start;
                // Use epsilon comparison for float ranges
                while (step > 0.0 && i <= stop + 1e-10) || (step < 0.0 && i >= stop - 1e-10) {
                    data.push(i);
                    i += step;
                }
                let len = data.len();
                let arr = ArrayValue::memory_first_from_f64(data, vec![len]);
                self.push_array_value_as_wrapper(arr)?;
                Ok(DispatchAction::Continue)
            }

            Instr::MakeRangeLazy | Instr::MakeStepRangeLazy => {
                // Create lazy Range value (does not materialize to array).
                // `MakeStepRangeLazy` is emitted for the explicit-step form `a:s:b`,
                // so the result is a `StepRange` even when the step is 1 (Issue #5667).
                let is_step_range = matches!(instr, Instr::MakeStepRangeLazy);
                // Detect if any operand is a float type BEFORE popping
                // Stack layout (top to bottom): stop, step, start
                // Issue #3550: also remember the operand element type so iteration
                // and `typeof` can preserve it (e.g. `UInt8(1):UInt8(3)`).
                // Issue #4795: also recognize Char operands for Char ranges
                // (`'a':'e'` → StepRange{Char, Int} in upstream Julia).
                let n = self.stack.len();
                let is_float = n >= 3
                    && [&self.stack[n - 1], &self.stack[n - 2], &self.stack[n - 3]]
                        .iter()
                        .any(|v| matches!(v, Value::F64(_) | Value::F32(_) | Value::F16(_)));
                let is_char_range = n >= 3
                    && (matches!(self.stack[n - 1], Value::Char(_))
                        || matches!(self.stack[n - 3], Value::Char(_)));
                let element_type = if n >= 3 {
                    let operands = [&self.stack[n - 1], &self.stack[n - 2], &self.stack[n - 3]];
                    derive_range_element_type(&operands)
                } else {
                    RangeElementType::Default
                };
                let element_type = if is_char_range {
                    RangeElementType::Char
                } else {
                    element_type
                };

                let stop = self.pop_f64_or_i64_or_char()?;
                let step = self.pop_f64_or_i64_or_char()?;
                let start = self.pop_f64_or_i64_or_char()?;
                let range = RangeValue {
                    start,
                    step,
                    stop,
                    is_float,
                    element_type,
                    is_step_range,
                };
                self.stack.push(Value::Range(range));
                Ok(DispatchAction::Continue)
            }

            _ => Err(super::unhandled(instr)),
        }
    }
}

/// Derive a [`RangeElementType`] tag from the typed integer/float operands of
/// a `start:step:stop` range. The operands are passed in the same order they
/// appear on the VM stack (top, mid, bottom) — i.e. `stop`, `step`, `start`.
///
/// We require all *typed* integer operands to share the same width before
/// returning a non-default tag. `Int64`/`Float64` operands behave as the
/// historical default. Float typed operands (`Float32`) propagate similarly.
fn derive_range_element_type(operands: &[&Value; 3]) -> RangeElementType {
    let mut tag: Option<RangeElementType> = None;
    for v in operands {
        let t = match v {
            Value::I8(_) => Some(RangeElementType::Int8),
            Value::I16(_) => Some(RangeElementType::Int16),
            Value::I32(_) => Some(RangeElementType::Int32),
            Value::I64(_) => None, // default — does not narrow the tag
            Value::U8(_) => Some(RangeElementType::UInt8),
            Value::U16(_) => Some(RangeElementType::UInt16),
            Value::U32(_) => Some(RangeElementType::UInt32),
            Value::U64(_) => Some(RangeElementType::UInt64),
            Value::F32(_) => Some(RangeElementType::Float32),
            Value::F64(_) => None, // default float branch handled separately
            _ => return RangeElementType::Default,
        };
        if let Some(new_tag) = t {
            match tag {
                None => tag = Some(new_tag),
                Some(prev) if prev == new_tag => {}
                Some(_) => return RangeElementType::Default,
            }
        }
    }
    tag.unwrap_or(RangeElementType::Default)
}
