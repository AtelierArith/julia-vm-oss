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
use super::super::util::format_float_julia;
use super::super::value::{ArrayValue, RangeElementType, RangeValue, RustBigInt, Value};
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
                    derive_range_element_type(&operands, is_step_range)
                } else {
                    RangeElementType::Default
                };
                let element_type = if is_char_range {
                    RangeElementType::Char
                } else {
                    element_type
                };
                let step_type = if n >= 3 {
                    derive_range_step_type(&self.stack[n - 2])
                } else if is_char_range {
                    RangeElementType::Default
                } else {
                    element_type
                };

                let range = if matches!(element_type, RangeElementType::BigInt) && !is_float {
                    let stop = range_operand_to_bigint(self.stack.pop_value()?)?;
                    let step = range_operand_to_bigint(self.stack.pop_value()?)?;
                    let start = range_operand_to_bigint(self.stack.pop_value()?)?;
                    RangeValue::bigint_range(
                        start,
                        step,
                        stop,
                        is_step_range,
                        element_type,
                        step_type,
                    )
                } else {
                    let stop = self.pop_f64_or_i64_or_char()?;
                    let step = self.pop_f64_or_i64_or_char()?;
                    let start = self.pop_f64_or_i64_or_char()?;
                    RangeValue {
                        start,
                        step,
                        stop,
                        is_float,
                        element_type,
                        step_type,
                        is_step_range,
                        linspace_len: None,
                        step_defined: false,
                        bigint: None,
                    }
                };
                self.stack.push(Value::Range(range));
                Ok(DispatchAction::Continue)
            }

            Instr::CoerceRangeStopI64 => {
                // Coerce a range's runtime `stop` to `Int64` with upstream
                // last-element semantics so an integer-typed range whose bound
                // arrives as a Float truncates toward the iteration direction
                // instead of erroring (Issue #9321).
                //
                // Stack (top → down): `stop`, `step`, `start`. `step` and
                // `start` are peeked (not popped); `stop` is popped and replaced
                // on top by its integer coercion.
                //
                // Issue #9377: upstream computes `q = (stop - start) / step` —
                // if `q < 0` the range is empty (length 0, no error), otherwise
                // `floor(Int, q)` raises `InexactError` for a `NaN` / `±Inf` /
                // out-of-`Int64`-range result. Mirror that here: a non-finite or
                // out-of-range bound in the *counting* direction raises a
                // catchable `InexactError` (`length(1:Inf)` errors), while the
                // *empty* direction stays error-free (`length(1:-Inf) == 0`).
                let n = self.stack.len();
                if n < 3 {
                    // INTERNAL: the compiler always pushes `start` and `step`
                    // beneath `stop` before emitting this instruction — a short
                    // stack is a codegen invariant violation, not a user error.
                    return Err(VmError::InternalError(
                        "CoerceRangeStopI64: expected `start`, `step` below `stop` on the stack"
                            .to_string(),
                    ));
                }
                // Read a numeric stack operand as `f64` for the direction test.
                // A non-numeric value yields `None` (callers compile `start` as
                // `I64` and validate `step` when the range itself is built).
                let numeric_f64 = |v: &Value| -> Option<f64> {
                    match v {
                        Value::I64(v) => Some(*v as f64),
                        Value::I8(v) => Some(f64::from(*v)),
                        Value::I16(v) => Some(f64::from(*v)),
                        Value::I32(v) => Some(f64::from(*v)),
                        Value::I128(v) => Some(*v as f64),
                        Value::U8(v) => Some(f64::from(*v)),
                        Value::U16(v) => Some(f64::from(*v)),
                        Value::U32(v) => Some(f64::from(*v)),
                        Value::U64(v) => Some(*v as f64),
                        Value::U128(v) => Some(*v as f64),
                        Value::Bool(b) => Some(f64::from(u8::from(*b))),
                        Value::F64(v) => Some(*v),
                        Value::F32(v) => Some(f64::from(*v)),
                        Value::F16(v) => Some(v.to_f64()),
                        _ => None,
                    }
                };
                // A negative step counts downward, so the last reachable integer
                // is `ceil(stop)`; otherwise `floor(stop)`. A non-numeric step is
                // treated as ascending (floor) — the step type is validated when
                // the range itself is built. A zero step keeps the pre-#9377
                // rounding-only behavior; the range construction / loop itself
                // rejects zero steps.
                let step_f = numeric_f64(&self.stack[n - 2]).unwrap_or(1.0);
                let start_f = numeric_f64(&self.stack[n - 3]);
                let step_is_negative = step_f < 0.0;
                // The `stop` value that makes the range empty regardless of the
                // (integer) `start`: one step "before" the start in the counting
                // direction. Used when the true bound is not representable as
                // `i64` but the direction is legally empty (`1:-Inf`).
                let empty_stop = || -> i64 {
                    let start_i = start_f.map_or(0, |s| s as i64);
                    if step_is_negative {
                        start_i.saturating_add(1)
                    } else {
                        start_i.saturating_sub(1)
                    }
                };
                // `f64` bounds of the `Int64` domain: `-(2^63)` is exactly
                // representable; `2^63` (== `i64::MAX as f64`, rounded up) is the
                // first excluded value, so the valid interval is half-open.
                let fits_i64 = |v: f64| -> bool {
                    v.is_finite() && v >= (i64::MIN as f64) && v < -(i64::MIN as f64)
                };
                // Round a float bound toward the counting direction, raising the
                // upstream `InexactError` when the bound is `NaN` or the rounded
                // value cannot be represented as `Int64` while the range counts
                // toward it (`q = (stop - start) / step >= 0`); an empty
                // direction (`q < 0`) never errors (Issue #9377).
                let round = |v: f64| -> Result<i64, VmError> {
                    let rounded = if step_is_negative {
                        v.ceil()
                    } else {
                        v.floor()
                    };
                    if fits_i64(rounded) {
                        return Ok(rounded as i64);
                    }
                    if v.is_nan() {
                        return Err(VmError::InexactError(format!(
                            "Int64({})",
                            format_float_julia(v)
                        )));
                    }
                    match (start_f, step_f != 0.0) {
                        (Some(start), true) => {
                            let q = (v - start) / step_f;
                            if q < 0.0 {
                                // Legal empty direction: any empty Int range.
                                Ok(empty_stop())
                            } else {
                                Err(VmError::InexactError(format!(
                                    "Int64({})",
                                    format_float_julia(v)
                                )))
                            }
                        }
                        // Zero step (rejected later by the range itself) or a
                        // non-numeric start (codegen guarantees `I64`): keep the
                        // pre-#9377 saturating cast.
                        _ => Ok(rounded as i64),
                    }
                };
                let coerced: Result<i64, VmError> = match self.stack.pop_value()? {
                    Value::I64(v) => Ok(v),
                    Value::I8(v) => Ok(i64::from(v)),
                    Value::I16(v) => Ok(i64::from(v)),
                    Value::I32(v) => Ok(i64::from(v)),
                    Value::I128(v) => Ok(v as i64),
                    Value::U8(v) => Ok(i64::from(v)),
                    Value::U16(v) => Ok(i64::from(v)),
                    Value::U32(v) => Ok(i64::from(v)),
                    Value::U64(v) => Ok(v as i64),
                    Value::U128(v) => Ok(v as i64),
                    Value::Bool(b) => Ok(i64::from(b)),
                    Value::F64(v) => round(v),
                    Value::F32(v) => round(f64::from(v)),
                    Value::F16(v) => round(v.to_f64()),
                    Value::Char(c) => Ok(c as i64),
                    Value::BigInt(ref b) => match b.to_string().parse::<i64>() {
                        Ok(v) => Ok(v),
                        Err(_) => {
                            // Out-of-`Int64` `BigInt` bound: counting toward it
                            // raises `InexactError` (`1:big(10)^30`); the empty
                            // direction gives length 0 (`1:-big(10)^30`,
                            // `10:-2:big(10)^30`), matching upstream.
                            let stop_positive = *b > crate::vm::value::RustBigInt::from(0);
                            let counting = stop_positive != step_is_negative;
                            if counting {
                                Err(VmError::InexactError(format!("Int64({b})")))
                            } else {
                                Ok(empty_stop())
                            }
                        }
                    },
                    Value::BigFloat(ref b) => round(b.to_string().parse::<f64>().unwrap_or(0.0)),
                    other => {
                        // User-visible: a range bound that is neither integer nor
                        // float (e.g. a String) — upstream has no matching `(:)`
                        // method, so surface a runtime TypeError.
                        return Err(VmError::type_error_expected(
                            "CoerceRangeStopI64",
                            "numeric",
                            &other,
                        ));
                    }
                };
                match coerced {
                    Ok(v) => {
                        self.stack.push(Value::I64(v));
                    }
                    Err(err @ VmError::InexactError(_)) => {
                        // Route through `raise` so an enclosing `try`/`catch`
                        // observes the upstream-shaped `InexactError` (the run
                        // loop propagates a bare `Err` uncaught); `handle_error`
                        // truncates the leftover `start`/`step` operands to the
                        // handler's saved stack depth (Issue #9377).
                        self.raise(err)?;
                    }
                    Err(other) => return Err(other),
                }
                Ok(DispatchAction::Continue)
            }

            _ => Err(super::unhandled(instr)),
        }
    }
}

fn range_operand_to_bigint(value: Value) -> Result<RustBigInt, VmError> {
    match value {
        Value::BigInt(v) => Ok(v),
        Value::I8(v) => Ok(RustBigInt::from(v)),
        Value::I16(v) => Ok(RustBigInt::from(v)),
        Value::I32(v) => Ok(RustBigInt::from(v)),
        Value::I64(v) => Ok(RustBigInt::from(v)),
        Value::I128(v) => Ok(RustBigInt::from(v)),
        Value::U8(v) => Ok(RustBigInt::from(v)),
        Value::U16(v) => Ok(RustBigInt::from(v)),
        Value::U32(v) => Ok(RustBigInt::from(v)),
        Value::U64(v) => Ok(RustBigInt::from(v)),
        Value::U128(v) => Ok(RustBigInt::from(v)),
        other => Err(VmError::TypeError(format!(
            "BigInt range operand must be integer, got {:?}",
            other
        ))),
    }
}

/// Derive a [`RangeElementType`] tag from the typed integer/float operands of
/// a `start:step:stop` range. The operands are passed in the same order they
/// appear on the VM stack (top, mid, bottom) — i.e. `stop`, `step`, `start`.
///
/// The tag is the **promotion join** of `start`, `step`, and `stop`, mirroring
/// `Base.colon`, which promotes all three components together before building
/// the `StepRangeLen`/`StepRange` (`base/range.jl`; Issue #9322). In particular:
///
/// * **Float path** — if *any* operand is a floating-point value, the element
///   type is the widest float present (`Float64` > `Float32` > `Float16`),
///   exactly like `promote_type`: integers promote up to whatever float is
///   present and never lower the float width. So `0:0.5f0:6.0`
///   (Int + Float32 + Float64) is a `Float64` range, not `Float32`.
///   Pure `Float16` / `Float16`-plus-integer ranges keep `Float16` as their
///   visible element type, with boxed Float16 array storage (Issue #10019).
/// * **Integer path** — with no float operands, the tag is the integer
///   *promotion join* of the endpoint operands, mirroring upstream
///   `(:)(start, stop) = (:)(promote(start, stop)...)` /
///   `(:)(start, step, stop) = (:)(promote(start, step, stop)...)`. Concretely
///   the widest integer of the shared signedness wins: `1:Int8(5)` promotes
///   `Int64`+`Int8` → `Int64` (a `UnitRange{Int64}`, Issue #9345), while
///   `UInt8(1):UInt8(3)` stays `UInt8` and `Int8(1):Int16(5)` widens to `Int16`.
///   For a **unit range** (`a:b`, `is_step_range == false`) the middle operand
///   is the synthetic `Int64` unit step, not a user endpoint, so it is excluded
///   from the join — only `start`/`stop` participate, exactly as upstream
///   promotes just the two endpoints. Mixed signed/unsigned operands or an
///   `Int64`-wide result fall back to `Default` (which renders `Int64`).
fn derive_range_element_type(operands: &[&Value; 3], is_step_range: bool) -> RangeElementType {
    // Float path: floats dominate integers under `promote_type`, and among the
    // floats present the widest wins. Rank: Float16 = 0, Float32 = 1,
    // Float64 = 2. This is `promote_type` restricted to the float widths.
    let mut float_rank: Option<u8> = None;
    for v in operands {
        let rank = match v {
            Value::F16(_) => Some(0u8),
            Value::F32(_) => Some(1u8),
            Value::F64(_) => Some(2u8),
            _ => None,
        };
        if let Some(rank) = rank {
            float_rank = Some(float_rank.map_or(rank, |cur| cur.max(rank)));
        }
    }
    if let Some(rank) = float_rank {
        return match rank {
            2 => RangeElementType::Float64,
            1 => RangeElementType::Float32,
            _ => RangeElementType::Float16,
        };
    }

    // Integer path (no float operands): compute the integer promotion join of
    // the endpoint operands. For a unit range the middle operand is the
    // synthetic `Int64` unit step (`sjulia` emits a generic `Int64` step for
    // `a:b`), which is not a user endpoint and must not participate — upstream
    // `(:)(a, b)` promotes only `a` and `b`. Step ranges promote all three.
    let mut join = IntClass::Neutral;
    let relevant: &[&Value] = if is_step_range {
        &operands[..]
    } else {
        // Exclude the middle (synthetic step) operand: [stop, start].
        &[operands[0], operands[2]]
    };
    for v in relevant {
        join = join.join(IntClass::classify(v));
        if matches!(join, IntClass::Fallback) {
            return RangeElementType::Default;
        }
    }
    join.to_element_type()
}

fn derive_range_step_type(step: &Value) -> RangeElementType {
    match step {
        Value::I8(_) => RangeElementType::Int8,
        Value::I16(_) => RangeElementType::Int16,
        Value::I32(_) => RangeElementType::Int32,
        Value::I64(_) => RangeElementType::Default,
        Value::U8(_) => RangeElementType::UInt8,
        Value::U16(_) => RangeElementType::UInt16,
        Value::U32(_) => RangeElementType::UInt32,
        Value::U64(_) => RangeElementType::UInt64,
        Value::F16(_) => RangeElementType::Float16,
        Value::F32(_) => RangeElementType::Float32,
        Value::F64(_) => RangeElementType::Float64,
        Value::Char(_) => RangeElementType::Char,
        Value::BigInt(_) => RangeElementType::BigInt,
        _ => RangeElementType::Default,
    }
}

/// Signedness + width classification of a single range operand used to compute
/// the integer promotion join (`derive_range_element_type`). `Neutral` is the
/// join identity; `Fallback` (any non-integer operand, mixed signedness, or an
/// `Int64`-wide result) collapses to `RangeElementType::Default`.
#[derive(Clone, Copy)]
enum IntClass {
    /// No integer information yet (join identity).
    Neutral,
    /// Signed integer of the given bit width.
    Signed(u8),
    /// Unsigned integer of the given bit width.
    Unsigned(u8),
    /// `BigInt` operand: upstream `promote_type(BigInt, <:Integer)` is
    /// `BigInt`, so a `BigInt` endpoint dominates every fixed-width integer
    /// (Issue #9420).
    Big,
    /// Not representable as a narrow-int tag → render as `Int64`.
    Fallback,
}

impl IntClass {
    fn classify(v: &Value) -> IntClass {
        match v {
            Value::I8(_) => IntClass::Signed(8),
            Value::I16(_) => IntClass::Signed(16),
            Value::I32(_) => IntClass::Signed(32),
            Value::I64(_) => IntClass::Signed(64),
            Value::U8(_) => IntClass::Unsigned(8),
            Value::U16(_) => IntClass::Unsigned(16),
            Value::U32(_) => IntClass::Unsigned(32),
            Value::U64(_) => IntClass::Unsigned(64),
            // BigInt endpoint: `1:big(3)` must be a `UnitRange{BigInt}`
            // (Issue #9420) — upstream promotes both endpoints to `BigInt`.
            Value::BigInt(_) => IntClass::Big,
            // I128/U128/Bool/Char/etc.: no narrow tag — fall back to the
            // historical `Int64`/`Default` rendering.
            _ => IntClass::Fallback,
        }
    }

    /// Promotion join: widest wins within a shared signedness; a mix of signed
    /// and unsigned (or any `Fallback`) collapses to `Fallback`.
    fn join(self, other: IntClass) -> IntClass {
        match (self, other) {
            (IntClass::Fallback, _) | (_, IntClass::Fallback) => IntClass::Fallback,
            (IntClass::Neutral, x) | (x, IntClass::Neutral) => x,
            // BigInt absorbs every fixed-width integer, matching upstream
            // `promote_type(BigInt, <:Integer) == BigInt` (Issue #9420).
            (IntClass::Big, _) | (_, IntClass::Big) => IntClass::Big,
            (IntClass::Signed(a), IntClass::Signed(b)) => IntClass::Signed(a.max(b)),
            (IntClass::Unsigned(a), IntClass::Unsigned(b)) => IntClass::Unsigned(a.max(b)),
            // Mixed signed/unsigned: upstream `promote_type` widens to a signed
            // type that can hold both; sjulia has no narrow tag for that, so
            // fall back to `Int64`.
            _ => IntClass::Fallback,
        }
    }

    fn to_element_type(self) -> RangeElementType {
        match self {
            IntClass::Signed(8) => RangeElementType::Int8,
            IntClass::Signed(16) => RangeElementType::Int16,
            IntClass::Signed(32) => RangeElementType::Int32,
            IntClass::Unsigned(8) => RangeElementType::UInt8,
            IntClass::Unsigned(16) => RangeElementType::UInt16,
            IntClass::Unsigned(32) => RangeElementType::UInt32,
            IntClass::Unsigned(64) => RangeElementType::UInt64,
            IntClass::Big => RangeElementType::BigInt,
            // Signed 64-bit (or `Neutral`) renders as the default `Int64`.
            _ => RangeElementType::Default,
        }
    }
}

#[cfg(test)]
mod derive_range_element_type_tests {
    use super::{derive_range_element_type, derive_range_step_type, RangeElementType, Value};

    fn derive(stop: Value, step: Value, start: Value) -> RangeElementType {
        // Operands are passed in stack order: top (stop), mid (step), bottom (start).
        // The float tests below all use an explicit float step, so treat them as
        // step ranges (the `is_step_range` flag only gates the integer path).
        derive_range_element_type(&[&stop, &step, &start], true)
    }

    /// Unit-range form (`a:b`): the middle operand is the synthetic `Int64`
    /// unit step, which must be excluded from the integer promotion join.
    fn derive_unit(stop: Value, start: Value) -> RangeElementType {
        derive_range_element_type(&[&stop, &Value::I64(1), &start], false)
    }

    // ── Mixed float widths promote to the widest float (Issue #9322) ──────────

    #[test]
    fn int_float32_float64_promotes_to_float64() {
        // 0:0.5f0:6.0 — Int64 start, Float32 step, Float64 stop → Float64.
        assert_eq!(
            derive(Value::F64(6.0), Value::F32(0.5), Value::I64(0)),
            RangeElementType::Float64
        );
    }

    #[test]
    fn float32_float64_int_promotes_to_float64() {
        // 0f0:0.5:6 — Float32 start, Float64 step, Int64 stop → Float64.
        assert_eq!(
            derive(Value::I64(6), Value::F64(0.5), Value::F32(0.0)),
            RangeElementType::Float64
        );
    }

    #[test]
    fn pure_float32_stays_float32() {
        // 0f0:0.5f0:6f0 must NOT be widened to Float64.
        assert_eq!(
            derive(Value::F32(6.0), Value::F32(0.5), Value::F32(0.0)),
            RangeElementType::Float32
        );
    }

    #[test]
    fn int_float32_stays_float32() {
        // 0:0.5f0:6 — Int + Float32, no Float64 → Float32.
        assert_eq!(
            derive(Value::I64(6), Value::F32(0.5), Value::I64(0)),
            RangeElementType::Float32
        );
    }

    #[test]
    fn float16_plus_float32_promotes_to_float32() {
        // Float16(0):0.5f0:6 — widest float is Float32.
        assert_eq!(
            derive(
                Value::I64(6),
                Value::F32(0.5),
                Value::F16(half::f16::from_f32(0.0))
            ),
            RangeElementType::Float32
        );
    }

    #[test]
    fn pure_float16_stays_float16() {
        // Float16(0):Float16(0.5):Float16(6) stays Float16 and uses Float64
        // StepRangeLen accumulator fields upstream (Issue #10019).
        assert_eq!(
            derive(
                Value::F16(half::f16::from_f32(6.0)),
                Value::F16(half::f16::from_f32(0.5)),
                Value::F16(half::f16::from_f32(0.0))
            ),
            RangeElementType::Float16
        );
    }

    #[test]
    fn int_float16_stays_float16() {
        assert_eq!(
            derive(
                Value::I64(6),
                Value::F16(half::f16::from_f32(0.5)),
                Value::I64(0)
            ),
            RangeElementType::Float16
        );
    }

    #[test]
    fn float16_plus_float64_promotes_to_float64() {
        // Float16(0):Float16(0.5):6.0 — widest float is Float64.
        assert_eq!(
            derive(
                Value::F64(6.0),
                Value::F16(half::f16::from_f32(0.5)),
                Value::F16(half::f16::from_f32(0.0))
            ),
            RangeElementType::Float64
        );
    }

    #[test]
    fn pure_float64_is_float64() {
        assert_eq!(
            derive(Value::F64(6.0), Value::F64(0.5), Value::F64(0.0)),
            RangeElementType::Float64
        );
    }

    // ── Integer promotion join (Issue #9345) ─────────────────────────────────

    #[test]
    fn plain_int_range_is_default() {
        assert_eq!(
            derive(Value::I64(5), Value::I64(1), Value::I64(1)),
            RangeElementType::Default
        );
    }

    #[test]
    fn typed_uint8_range_keeps_uint8() {
        // UInt8(1):UInt8(3) — a unit range whose synthetic Int64 step is excluded.
        assert_eq!(
            derive_unit(Value::U8(3), Value::U8(1)),
            RangeElementType::UInt8
        );
    }

    #[test]
    fn int64_start_int8_stop_promotes_to_int64() {
        // 1:Int8(5) — promote(Int64, Int8) == Int64, so `UnitRange{Int64}`, NOT
        // `UnitRange{Int8}` (Issue #9345). The synthetic Int64 step is excluded,
        // but the explicit Int64 start still forces the Int64 join.
        assert_eq!(
            derive_unit(Value::I8(5), Value::I64(1)),
            RangeElementType::Default
        );
    }

    #[test]
    fn narrow_int_unit_range_widens_to_wider_endpoint() {
        // Int8(1):Int16(5) — promote(Int8, Int16) == Int16.
        assert_eq!(
            derive_unit(Value::I16(5), Value::I8(1)),
            RangeElementType::Int16
        );
    }

    #[test]
    fn same_width_signed_unit_range_keeps_width() {
        // Int8(1):Int8(5) — stays Int8.
        assert_eq!(
            derive_unit(Value::I8(5), Value::I8(1)),
            RangeElementType::Int8
        );
    }

    #[test]
    fn mixed_sign_int_range_falls_back_to_default() {
        // Int8 + UInt8 have no narrow shared tag → Default (Int64).
        assert_eq!(
            derive_unit(Value::U8(5), Value::I8(1)),
            RangeElementType::Default
        );
    }

    // ── BigInt endpoint promotion (Issue #9420) ───────────────────────────────

    #[test]
    fn bigint_stop_unit_range_is_bigint() {
        // 1:big(3) — promote(Int64, BigInt) == BigInt → UnitRange{BigInt}.
        assert_eq!(
            derive_unit(
                Value::BigInt(crate::vm::value::RustBigInt::from(3)),
                Value::I64(1)
            ),
            RangeElementType::BigInt
        );
    }

    #[test]
    fn bigint_start_unit_range_is_bigint() {
        // big(1):3 — BigInt start also forces the BigInt join.
        assert_eq!(
            derive_unit(
                Value::I64(3),
                Value::BigInt(crate::vm::value::RustBigInt::from(1))
            ),
            RangeElementType::BigInt
        );
    }

    #[test]
    fn bigint_step_range_promotes_all_three_operands() {
        // big(1):2:9 — the BigInt endpoint dominates the Int64 step/stop.
        assert_eq!(
            derive(
                Value::I64(9),
                Value::I64(2),
                Value::BigInt(crate::vm::value::RustBigInt::from(1))
            ),
            RangeElementType::BigInt
        );
    }

    #[test]
    fn bigint_with_float_stays_on_float_path() {
        // big(1):0.5:3.0 — floats dominate; upstream is BigFloat but sjulia has
        // no BigFloat range tag yet, so the float path's Float64 wins (the
        // pre-existing behaviour; see Issue #9420 follow-up).
        assert_eq!(
            derive(
                Value::F64(3.0),
                Value::F64(0.5),
                Value::BigInt(crate::vm::value::RustBigInt::from(1))
            ),
            RangeElementType::Float64
        );
    }

    #[test]
    fn explicit_step_type_preserves_original_step_width_issue_9519() {
        assert_eq!(
            derive_range_step_type(&Value::I64(2)),
            RangeElementType::Default
        );
        assert_eq!(
            derive_range_step_type(&Value::I8(2)),
            RangeElementType::Int8
        );
        assert_eq!(
            derive_range_step_type(&Value::F16(half::f16::from_f32(0.5))),
            RangeElementType::Float16
        );
        assert_eq!(
            derive_range_step_type(&Value::BigInt(crate::vm::value::RustBigInt::from(2))),
            RangeElementType::BigInt
        );
    }

    #[test]
    fn step_range_promotes_all_three_operands() {
        // Int8(1):Int8(1):Int16(5) — the step participates → widest is Int16.
        assert_eq!(
            derive(Value::I16(5), Value::I8(1), Value::I8(1)),
            RangeElementType::Int16
        );
    }
}
