//! RangeValue - Lazy range representation for Julia's `start:step:stop` syntax.
//!
//! This module contains the `RangeValue` struct for representing lazy ranges
//! that support both integer and floating-point ranges.

// SAFETY: f64→usize casts for range len() use `.floor()` on non-negative values
// (guarded by `if step > 0 && stop >= start` or vice versa); i64→usize for
// collect() capacity uses `length()` which returns ≥ 0.
#![allow(clippy::cast_sign_loss)]

use super::super::error::VmError;
use super::twiceprecision::{
    colon_hp, colon_hp_length, linspace1_hp, linspace_hp, steprangelen_hp_from_step, HpElement,
    RangeHp,
};
use super::{ArrayData, ArrayElementType, ArrayValue, RustBigInt};
use num_traits::{One, ToPrimitive, Zero};

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
    Float16,
    Float32,
    Float64,
    /// Char range, e.g. `'a':'e'`. Iteration yields `Value::Char` by
    /// converting the stored Unicode codepoint back via `char::from_u32`
    /// (Issue #4795).
    Char,
    /// `BigInt` endpoint range, e.g. `1:big(3)` — upstream promotes the
    /// endpoints to `BigInt`, so `typeof` is `UnitRange{BigInt}` and
    /// iteration yields `BigInt` values (Issue #9420).
    BigInt,
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
            RangeElementType::Float16 => "Float16",
            RangeElementType::Float32 => "Float32",
            RangeElementType::Float64 => "Float64",
            RangeElementType::Char => "Char",
            RangeElementType::BigInt => "BigInt",
        }
    }

    /// Whether the tagged element type is a floating-point type.
    pub fn is_float(&self) -> bool {
        matches!(
            self,
            RangeElementType::Float16 | RangeElementType::Float32 | RangeElementType::Float64
        )
    }
}

/// Exact endpoint storage for `UnitRange{BigInt}` / `StepRange{BigInt,S}`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct BigIntRangeParts {
    pub start: RustBigInt,
    pub step: RustBigInt,
    pub stop: RustBigInt,
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
    /// Original element type of the promoted range values, retained for typed
    /// integer ranges (`UInt8(1):UInt8(3)` etc.). Used by `typeof` and
    /// iteration. Issue #3550.
    pub element_type: RangeElementType,
    /// Original type of the user-provided explicit step for `StepRange{T,S}`.
    /// `element_type` is the promoted range element type, while `step_type`
    /// preserves `S` (Issue #9519).
    pub step_type: RangeElementType,
    /// True when the range was written with an explicit step (`a:s:b`), so it is a
    /// `StepRange` even if the step is 1. Upstream distinguishes `1:1:5`
    /// (`StepRange`) from `1:5` (`UnitRange`); without this flag a step of 1 always
    /// looked like a `UnitRange` (Issue #5667).
    pub is_step_range: bool,
    /// `Some(len)` marks a length-defined float range built by
    /// `range(start, stop; length = len)` (Issue #9419). The length is
    /// authoritative (it overrides the `(stop - start) / step` derivation) and
    /// elements are materialized with upstream `_linspace` TwicePrecision
    /// semantics (`first(r) == start`, `last(r) == stop`, exactly `len`
    /// elements). `None` for ordinary colon ranges.
    pub linspace_len: Option<i64>,
    /// True when `linspace_len` came from `range(start; step, length)`
    /// (Issue #9509): the high-precision parts derive from `(start, step,
    /// len)` via upstream `range_start_step_length` / `floatrange` instead of
    /// the endpoint `_linspace`. Always false when `linspace_len` is `None`.
    pub step_defined: bool,
    /// Exact endpoint storage for BigInt-promoted ranges. The legacy `f64`
    /// fields remain populated for type/display fingerprints and non-BigInt
    /// callers, but element materialization uses these exact values when set.
    pub bigint: Option<Box<BigIntRangeParts>>,
}

impl RangeValue {
    fn inclusive_step_count(distance: f64, step: f64) -> i64 {
        const FLOAT_RANGE_ENDPOINT_EPSILON: f64 = 1e-10;
        (((distance / step) + FLOAT_RANGE_ENDPOINT_EPSILON).floor() as i64).saturating_add(1)
    }

    /// Create a unit range (step = 1): start:stop
    pub fn unit_range(start: f64, stop: f64) -> Self {
        Self {
            start,
            step: 1.0,
            stop,
            is_float: false,
            element_type: RangeElementType::Default,
            step_type: RangeElementType::Default,
            is_step_range: false,
            linspace_len: None,
            step_defined: false,
            bigint: None,
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
            step_type: RangeElementType::Default,
            is_step_range: true,
            linspace_len: None,
            step_defined: false,
            bigint: None,
        }
    }

    /// The hp element kind for a float element-type tag (Issue #9509):
    /// Float16/Float32 collapse to plain Float64 ref/step, Float64 keeps
    /// the TwicePrecision representation.
    fn hp_element_for(element_type: RangeElementType) -> HpElement {
        if matches!(
            element_type,
            RangeElementType::Float16 | RangeElementType::Float32
        ) {
            HpElement::F32
        } else {
            HpElement::F64
        }
    }

    /// Create a length-defined float range for `range(start, stop; length)`
    /// (Issues #9419/#9509) — upstream's TwicePrecision-backed `StepRangeLen`
    /// from `range_start_stop_length(::T, ::T, ::Integer) where T<:IEEEFloat`.
    /// `element_type` must be a float tag (`Float64`, `Float32`, or
    /// `Float16`).
    ///
    /// The caller must have validated `len >= 0` and, for `len == 1`,
    /// `start == stop` (upstream `_linspace1` argument errors).
    pub fn float_linspace(start: f64, stop: f64, len: i64, element_type: RangeElementType) -> Self {
        let t = Self::hp_element_for(element_type);
        let hp = if len < 2 {
            linspace1_hp(t, start, stop, len)
        } else {
            linspace_hp(t, start, stop, len)
        };
        Self {
            start,
            // The user-visible step, `T(r.step)` — what `step(r)` and the
            // `start:step:stop` show form report.
            step: hp.step_f64(),
            stop,
            is_float: true,
            element_type,
            step_type: element_type,
            is_step_range: true,
            linspace_len: Some(len.max(0)),
            step_defined: false,
            bigint: None,
        }
    }

    /// Create a length-defined float range for `range(start; step, length)`
    /// (Issue #9509) — upstream's TwicePrecision-backed `StepRangeLen` from
    /// `range_start_step_length(::T, ::T, ::Integer) where T<:IEEEFloat`.
    /// `element_type` must be a float tag. The caller must have validated
    /// `len >= 0`.
    pub fn float_steplen(start: f64, step: f64, len: i64, element_type: RangeElementType) -> Self {
        let t = Self::hp_element_for(element_type);
        let len = len.max(0);
        let hp = steprangelen_hp_from_step(t, start, step, len);
        Self {
            start,
            step,
            // The display/`last` endpoint, `r[len]` (upstream `last(r)`
            // evaluates the hp lerp even for the empty range's show form).
            stop: hp.elem(len),
            is_float: true,
            element_type,
            step_type: element_type,
            is_step_range: true,
            linspace_len: Some(len),
            step_defined: true,
            bigint: None,
        }
    }

    pub fn bigint_range(
        start: RustBigInt,
        step: RustBigInt,
        stop: RustBigInt,
        is_step_range: bool,
        element_type: RangeElementType,
        step_type: RangeElementType,
    ) -> Self {
        let start_f = start.as_inner().to_f64().unwrap_or_else(|| {
            if start.as_inner() < &num_bigint::BigInt::zero() {
                f64::NEG_INFINITY
            } else {
                f64::INFINITY
            }
        });
        let step_f = step.as_inner().to_f64().unwrap_or_else(|| {
            if step.as_inner() < &num_bigint::BigInt::zero() {
                f64::NEG_INFINITY
            } else {
                f64::INFINITY
            }
        });
        let stop_f = stop.as_inner().to_f64().unwrap_or_else(|| {
            if stop.as_inner() < &num_bigint::BigInt::zero() {
                f64::NEG_INFINITY
            } else {
                f64::INFINITY
            }
        });
        Self {
            start: start_f,
            step: step_f,
            stop: stop_f,
            is_float: false,
            element_type,
            step_type,
            is_step_range,
            linspace_len: None,
            step_defined: false,
            bigint: Some(Box::new(BigIntRangeParts { start, step, stop })),
        }
    }

    /// TwicePrecision parts for a float range, when applicable (Issues
    /// #9419/#9421). Integer and Char ranges materialize exactly with plain
    /// arithmetic and return `None`.
    ///
    /// The parts are a pure function of `(start, step, stop, element_type,
    /// linspace_len)`; a thread-local single-entry memo makes per-element
    /// callers (`get` during iteration/indexing) derive them once per range
    /// instead of once per element. The VM is single-threaded by design
    /// (docs/vm/SINGLE_THREADED_VM.md), so `thread_local!` state is fine.
    pub fn float_hp(&self) -> Option<RangeHp> {
        if matches!(self.element_type, RangeElementType::Char) {
            return None;
        }
        let lin = self.linspace_len;
        if lin.is_none() && !self.is_float && self.is_integer_range() {
            return None;
        }

        thread_local! {
            static HP_MEMO: core::cell::Cell<Option<(HpKey, RangeHp)>> =
                const { core::cell::Cell::new(None) };
        }
        type HpKey = (u64, u64, u64, i64, bool, bool);
        let is_narrow_float = matches!(
            self.element_type,
            RangeElementType::Float16 | RangeElementType::Float32
        );
        let key: HpKey = (
            self.start.to_bits(),
            self.step.to_bits(),
            self.stop.to_bits(),
            lin.unwrap_or(-1),
            is_narrow_float,
            self.step_defined,
        );
        if let Some((cached_key, hp)) = HP_MEMO.with(core::cell::Cell::get) {
            if cached_key == key {
                return Some(hp);
            }
        }
        let t = if is_narrow_float {
            HpElement::F32
        } else {
            HpElement::F64
        };
        let hp = if let Some(len) = lin {
            if self.step_defined {
                steprangelen_hp_from_step(t, self.start, self.step, len)
            } else if len < 2 {
                linspace1_hp(t, self.start, self.stop, len)
            } else {
                linspace_hp(t, self.start, self.stop, len)
            }
        } else {
            colon_hp(t, self.start, self.step, self.stop, self.length())
        };
        HP_MEMO.with(|memo| memo.set(Some((key, hp))));
        Some(hp)
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

    pub fn element_type_name(&self) -> &'static str {
        match self.element_type {
            RangeElementType::Default => {
                if self.is_float {
                    "Float64"
                } else {
                    "Int64"
                }
            }
            other => other.julia_type_name(),
        }
    }

    pub fn is_explicit_float_type(&self) -> bool {
        self.element_type.is_float()
            || (matches!(self.element_type, RangeElementType::Default) && self.is_float)
    }

    pub fn julia_type_name(&self) -> String {
        let elem_name = self.element_type_name();
        if self.is_explicit_float_type() {
            let accumulator = match self.element_type {
                RangeElementType::Float16 | RangeElementType::Float32 => "Float64".to_string(),
                _ => format!("Base.TwicePrecision{{{elem_name}}}"),
            };
            format!("StepRangeLen{{{elem_name}, {accumulator}, {accumulator}, Int64}}")
        } else if matches!(self.element_type, RangeElementType::Char) {
            // Char ranges are StepRange values even for `a:b`. Preserve the
            // actual explicit-step type when one was provided (Issue #9519).
            format!("StepRange{{Char, {}}}", self.step_type.julia_type_name())
        } else if self.is_unit_range() {
            format!("UnitRange{{{}}}", elem_name)
        } else {
            format!(
                "StepRange{{{}, {}}}",
                elem_name,
                self.step_type.julia_type_name()
            )
        }
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
        if let Some(len) = self.bigint_length() {
            return len.to_i64().unwrap_or(i64::MAX);
        }
        // Length-defined ranges (`range(start, stop; length)`) carry their
        // exact length; never re-derive it from the float endpoints.
        if let Some(len) = self.linspace_len {
            return len;
        }
        let narrow_float_colon_len = || {
            if matches!(
                self.element_type,
                RangeElementType::Float16 | RangeElementType::Float32
            ) {
                colon_hp_length(HpElement::F32, self.start, self.step, self.stop)
            } else {
                None
            }
        };
        if self.step > 0.0 {
            if self.stop < self.start {
                0
            } else {
                narrow_float_colon_len().unwrap_or_else(|| {
                    Self::inclusive_step_count(self.stop - self.start, self.step)
                })
            }
        } else if self.step < 0.0 {
            if self.stop > self.start {
                0
            } else {
                narrow_float_colon_len().unwrap_or_else(|| {
                    Self::inclusive_step_count(self.start - self.stop, -self.step)
                })
            }
        } else {
            // step == 0 is invalid
            0
        }
    }

    fn bigint_length(&self) -> Option<num_bigint::BigInt> {
        let parts = self.bigint.as_ref()?;
        let start = parts.start.as_inner();
        let step = parts.step.as_inner();
        let stop = parts.stop.as_inner();
        if step.is_zero() {
            return Some(num_bigint::BigInt::zero());
        }
        if step > &num_bigint::BigInt::zero() {
            if stop < start {
                Some(num_bigint::BigInt::zero())
            } else {
                Some((stop - start) / step + num_bigint::BigInt::one())
            }
        } else if stop > start {
            Some(num_bigint::BigInt::zero())
        } else {
            Some((start - stop) / (-step) + num_bigint::BigInt::one())
        }
    }

    pub fn length_value(&self) -> super::Value {
        match self.bigint_length() {
            Some(len) => super::Value::BigInt(RustBigInt::from(len)),
            None => super::Value::I64(self.length()),
        }
    }

    /// Get element at 1-based index without allocating.
    ///
    /// Float ranges are evaluated with TwicePrecision semantics so
    /// `(0:0.1:1)[4] == 0.3`, matching upstream `StepRangeLen` getindex
    /// (Issue #9421); integer/Char ranges use plain (exact) arithmetic.
    pub fn get(&self, index: i64) -> Result<f64, VmError> {
        let len = self.length();
        if index < 1 || index > len {
            return Err(VmError::RangeIndexOutOfBounds { index, length: len });
        }
        if let Some(hp) = self.float_hp() {
            return Ok(hp.elem(index));
        }
        Ok(self.start + (index - 1) as f64 * self.step)
    }

    pub fn get_value(&self, index: i64) -> Result<super::Value, VmError> {
        let len = self.length();
        if index < 1 || index > len {
            return Err(VmError::RangeIndexOutOfBounds { index, length: len });
        }
        if let Some(parts) = &self.bigint {
            let offset = num_bigint::BigInt::from(index - 1);
            let value = parts.start.as_inner() + parts.step.as_inner() * offset;
            return Ok(super::Value::BigInt(RustBigInt::from(value)));
        }
        let val = self.get(index)?;
        Ok(self.typed_element(val))
    }

    /// Get the first element.
    pub fn first(&self) -> Option<f64> {
        if self.length() > 0 {
            Some(self.start)
        } else {
            None
        }
    }

    pub fn first_value(&self) -> Option<super::Value> {
        if let Some(parts) = &self.bigint {
            return Some(super::Value::BigInt(parts.start.clone()));
        }
        Some(self.typed_element(self.start))
    }

    /// Get the last element.
    pub fn last(&self) -> Option<f64> {
        let len = self.length();
        if len > 0 {
            // Route through `get` so float ranges report the TwicePrecision
            // last element (`last(0:0.1:1) == 1.0` exactly, Issue #9421).
            self.get(len).ok()
        } else {
            None
        }
    }

    pub fn last_value(&self) -> Option<super::Value> {
        if let Some(parts) = &self.bigint {
            let len = self.bigint_length()?;
            if len.is_zero() {
                return Some(super::Value::BigInt(parts.stop.clone()));
            }
            let offset = len - num_bigint::BigInt::one();
            let value = parts.start.as_inner() + parts.step.as_inner() * offset;
            return Some(super::Value::BigInt(RustBigInt::from(value)));
        }
        let val = self.last().unwrap_or(self.stop);
        Some(self.typed_element(val))
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
        // Issue #9420: BigInt ranges (`1:big(3)`) materialize BigInt elements
        // in boxed `Any` storage with the logical `BigInt` element tag, so
        // `collect(1:big(3))` is a `Vector{BigInt}` of `BigInt` values as in
        // upstream Julia (there is no dedicated BigInt array storage variant).
        if matches!(self.element_type, RangeElementType::BigInt) {
            let cap = if len > 0 { len as usize } else { 0 };
            let mut data: Vec<super::Value> = Vec::with_capacity(cap);
            if let Some(parts) = &self.bigint {
                for i in 0..len {
                    let offset = num_bigint::BigInt::from(i);
                    let v = parts.start.as_inner() + parts.step.as_inner() * offset;
                    data.push(super::Value::BigInt(RustBigInt::from(v)));
                }
            } else {
                for i in 0..len {
                    let v = (self.start + i as f64 * self.step) as i64;
                    data.push(super::Value::BigInt(RustBigInt::from(v)));
                }
            }
            let shape = vec![data.len()];
            let mem = super::memory_value::MemoryValue::new(
                super::array_data::ArrayData::Any(data),
                super::array_element::ArrayElementType::Abstract("BigInt".to_string()),
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
        } else if matches!(self.element_type, RangeElementType::Float16) {
            // Float16 ranges use upstream's narrow StepRangeLen layout:
            // element type Float16 with Float64 ref/step fields. The array
            // storage is boxed Any tagged as Float16 (Issue #9301).
            let cap = if len > 0 { len as usize } else { 0 };
            let hp = self.float_hp();
            let mut data = Vec::with_capacity(cap);
            for i in 0..len {
                let elem = match &hp {
                    Some(hp) => hp.elem(i + 1),
                    None => self.start + i as f64 * self.step,
                };
                data.push(super::Value::F16(half::f16::from_f64(elem)));
            }
            let shape = vec![data.len()];
            let mem = super::memory_value::MemoryValue::new(
                ArrayData::Any(data),
                ArrayElementType::F16,
                shape[0],
            );
            ArrayValue::from_memory(mem, shape)
        } else if matches!(self.element_type, RangeElementType::Float32) {
            // Float32 range: keep the element storage at Float32 while using the
            // same TwicePrecision materialization oracle as indexing/last
            // (Issues #9510/#9815).
            if len <= 0 {
                return ArrayValue::memory_first_from_array_data_with_element_type(
                    ArrayData::F32(vec![]),
                    vec![0],
                    ArrayElementType::F32,
                );
            }
            let hp = self.float_hp();
            let mut data = Vec::with_capacity(len as usize);
            for i in 0..len {
                let elem = match &hp {
                    Some(hp) => hp.elem(i + 1),
                    None => self.start + i as f64 * self.step,
                };
                data.push(elem as f32);
            }
            let len = data.len();
            ArrayValue::memory_first_from_array_data_with_element_type(
                ArrayData::F32(data),
                vec![len],
                ArrayElementType::F32,
            )
        } else if self.element_type.is_float() || self.is_float {
            // Float64/default float range. Materialize through the
            // TwicePrecision parts so `collect(0:0.1:1)` yields the
            // shortest-decimal grid values (Issue #9421).
            if len <= 0 {
                return ArrayValue::memory_first_from_f64(vec![], vec![0]);
            }
            let hp = self.float_hp();
            let mut data = Vec::with_capacity(len as usize);
            for i in 0..len {
                match &hp {
                    Some(hp) => data.push(hp.elem(i + 1)),
                    None => data.push(self.start + i as f64 * self.step),
                }
            }
            let len = data.len();
            ArrayValue::memory_first_from_f64(data, vec![len])
        } else {
            // Empty non-default integer tags preserve their logical eltype via
            // boxed storage because only Int64 has a native integer vector
            // carrier today.
            let element_type = super::array_element::ArrayElementType::Abstract(
                self.element_type.julia_type_name().to_string(),
            );
            let mem = super::memory_value::MemoryValue::new(
                super::array_data::ArrayData::Any(Vec::new()),
                element_type,
                0,
            );
            ArrayValue::from_memory(mem, vec![0])
        }
    }

    /// Convert the range to a Vec<f64> (materializes the range).
    pub fn to_vec(&self) -> Vec<f64> {
        let len = self.len();
        if len == 0 {
            return vec![];
        }
        let hp = self.float_hp();
        let mut data = Vec::with_capacity(len);
        for i in 0..len {
            match &hp {
                Some(hp) => data.push(hp.elem(i as i64 + 1)),
                None => data.push(self.start + i as f64 * self.step),
            }
        }
        data
    }

    pub fn elements_equal(&self, other: &Self) -> bool {
        if self.bigint.is_some() && other.bigint.is_some() {
            let len = self.length();
            if len != other.length() {
                return false;
            }
            if len <= 0 {
                return true;
            }
            for index in 1..=len {
                match (self.get_value(index), other.get_value(index)) {
                    (Ok(super::Value::BigInt(left)), Ok(super::Value::BigInt(right)))
                        if left == right => {}
                    _ => return false,
                }
            }
            return true;
        }
        self.to_vec() == other.to_vec()
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
        if let Some(parts) = &self.bigint {
            let x = match value {
                Value::BigInt(b) => b.as_inner().clone(),
                Value::I64(n) => num_bigint::BigInt::from(*n),
                Value::I32(n) => num_bigint::BigInt::from(*n),
                Value::I16(n) => num_bigint::BigInt::from(*n),
                Value::I8(n) => num_bigint::BigInt::from(*n),
                Value::I128(n) => num_bigint::BigInt::from(*n),
                Value::U64(n) => num_bigint::BigInt::from(*n),
                Value::U32(n) => num_bigint::BigInt::from(*n),
                Value::U16(n) => num_bigint::BigInt::from(*n),
                Value::U8(n) => num_bigint::BigInt::from(*n),
                Value::U128(n) => num_bigint::BigInt::from(*n),
                Value::F64(v) if v.is_finite() && v.fract() == 0.0 => match v.to_i128() {
                    Some(n) => num_bigint::BigInt::from(n),
                    None => return false,
                },
                Value::F32(v) if v.is_finite() && v.fract() == 0.0 => match v.to_i128() {
                    Some(n) => num_bigint::BigInt::from(n),
                    None => return false,
                },
                Value::F16(v) if v.is_finite() && v.to_f64().fract() == 0.0 => {
                    match v.to_f64().to_i128() {
                        Some(n) => num_bigint::BigInt::from(n),
                        None => return false,
                    }
                }
                _ => return false,
            };
            let step = parts.step.as_inner();
            if step.is_zero() {
                return x == *parts.start.as_inner();
            }
            let len = match self.bigint_length() {
                Some(len) if !len.is_zero() => len,
                _ => return false,
            };
            let diff = x - parts.start.as_inner();
            let rem = diff.clone() % step;
            if !rem.is_zero() {
                return false;
            }
            let k = diff / step;
            return k >= num_bigint::BigInt::zero() && k < len;
        }
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
            // Issue #9420: BigInt members of BigInt-endpoint ranges
            // (`big(2) in 1:big(3)`). Values beyond f64 range are never
            // members of an f64-backed range.
            Value::BigInt(b) => match num_traits::ToPrimitive::to_f64(b.as_inner()) {
                Some(v) => v,
                None => return false,
            },
            _ => return false,
        };
        if self.step == 0.0 {
            return x == self.start;
        }
        let n = self.length();
        if n <= 0 {
            return false;
        }
        // Float ranges: elements are TwicePrecision-exact, so locate the
        // nearest index and verify the materialized element (`0.3 in 0:0.1:1`
        // is true upstream, Issue #9421). The quotient is only approximate,
        // but any member's quotient rounds to its exact index.
        if let Some(hp) = self.float_hp() {
            let k = ((x - self.start) / self.step).round_ties_even();
            if !k.is_finite() || k < 0.0 || k >= n as f64 {
                return false;
            }
            let mut elem = hp.elem(k as i64 + 1);
            match self.element_type {
                RangeElementType::Float16 => elem = half::f16::from_f64(elem).to_f64(),
                RangeElementType::Float32 => elem = f64::from(elem as f32),
                _ => {}
            }
            return elem == x;
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

    fn typed_value(&self, ty: RangeElementType, val: f64) -> super::Value {
        match ty {
            RangeElementType::Int8 => super::Value::I8(val as i8),
            RangeElementType::Int16 => super::Value::I16(val as i16),
            RangeElementType::Int32 => super::Value::I32(val as i32),
            RangeElementType::Int64 => super::Value::I64(val as i64),
            RangeElementType::UInt8 => super::Value::U8(val as u8),
            RangeElementType::UInt16 => super::Value::U16(val as u16),
            RangeElementType::UInt32 => super::Value::U32(val as u32),
            RangeElementType::UInt64 => super::Value::U64(val as u64),
            RangeElementType::Float16 => super::Value::F16(half::f16::from_f64(val)),
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
            RangeElementType::BigInt => {
                // Issue #9420: `1:big(3)` yields BigInt elements, matching
                // upstream endpoint promotion. The stored f64 endpoint is
                // exact for |v| < 2^53 (larger endpoints are deferred —
                // see the enum variant doc).
                super::Value::BigInt(super::RustBigInt::from(val as i64))
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

    /// Materialize a single element value (such as `first(r)` or the loop
    /// variable in iteration) at the declared element type. Issue #3550.
    pub fn typed_element(&self, val: f64) -> super::Value {
        self.typed_value(self.element_type, val)
    }

    /// Materialize the stored step at the user-provided step type `S` in
    /// `StepRange{T,S}`. This cannot be derived from adjacent elements for empty
    /// or single-element ranges, and it intentionally differs from `T` when
    /// endpoints promote more widely than the explicit step (Issue #9519).
    pub fn typed_step(&self) -> super::Value {
        if let Some(parts) = &self.bigint {
            let ty = if self.is_unit_range() && self.element_type != RangeElementType::Char {
                self.element_type
            } else {
                self.step_type
            };
            if matches!(ty, RangeElementType::BigInt) {
                return super::Value::BigInt(parts.step.clone());
            }
            return self.typed_value(ty, self.step);
        }
        if self.is_unit_range() && self.element_type != RangeElementType::Char {
            return self.typed_value(self.element_type, self.step);
        }
        self.typed_value(self.step_type, self.step)
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
    fn test_length_saturates_at_i64_max_issue_11640() {
        let r = RangeValue::unit_range(1.0, i64::MAX as f64);
        assert_eq!(r.length(), i64::MAX);
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
        // TwicePrecision materialization (Issue #9421): the included endpoint
        // is the shortest-decimal 0.3, not the naive 0.1 + 2*0.1 accumulation
        // (0.30000000000000004) this test previously asserted.
        assert_eq!(r.get(4).unwrap(), 0.3);
    }

    #[test]
    fn test_len_of_float32_step_range_uses_float32_semantics_issue_9510() {
        let r = RangeValue {
            is_float: true,
            element_type: RangeElementType::Float32,
            ..RangeValue::step_range(f64::from(0.1f32), f64::from(0.1f32), f64::from(0.5f32))
        };
        assert_eq!(r.length(), 5);
        assert_eq!(r.len(), 5);
        assert_eq!((r.get(5).unwrap() as f32).to_bits(), 0.5f32.to_bits());
        assert_eq!(
            r.last().map(|v| (v as f32).to_bits()),
            Some(0.5f32.to_bits())
        );
        assert!(r.get(6).is_err());
    }

    // ── TwicePrecision float materialization (Issues #9419 / #9421) ──────────

    #[test]
    fn test_float_range_get_is_shortest_decimal_issue_9421() {
        let r = RangeValue {
            is_float: true,
            element_type: RangeElementType::Float64,
            ..RangeValue::step_range(0.0, 0.1, 1.0)
        };
        assert_eq!(r.len(), 11);
        assert_eq!(r.get(4).unwrap(), 0.3);
        assert_eq!(r.get(7).unwrap(), 0.6);
        assert_eq!(r.last(), Some(1.0));
        let collected = r.to_vec();
        assert_eq!(
            collected,
            vec![0.0, 0.1, 0.2, 0.3, 0.4, 0.5, 0.6, 0.7, 0.8, 0.9, 1.0]
        );
    }

    #[test]
    fn test_float_range_contains_grid_value_issue_9421() {
        let r = RangeValue {
            is_float: true,
            element_type: RangeElementType::Float64,
            ..RangeValue::step_range(0.0, 0.1, 1.0)
        };
        assert!(r.contains_value(&super::super::Value::F64(0.3)));
        assert!(!r.contains_value(&super::super::Value::F64(0.35)));
    }

    #[test]
    fn test_float_linspace_matches_upstream_issue_9419() {
        // range(0, 1, length = 3) — upstream 0.0:0.5:1.0.
        let r = RangeValue::float_linspace(0.0, 1.0, 3, RangeElementType::Float64);
        assert_eq!(r.length(), 3);
        assert_eq!(r.step, 0.5);
        assert_eq!(r.to_vec(), vec![0.0, 0.5, 1.0]);
        // range(0, 1, length = 7) — non-dyadic step, TwicePrecision grid.
        let r = RangeValue::float_linspace(0.0, 1.0, 7, RangeElementType::Float64);
        assert_eq!(r.length(), 7);
        assert_eq!(r.get(3).unwrap(), 0.3333333333333333);
        assert_eq!(r.last(), Some(1.0));
    }

    #[test]
    fn test_float_linspace_f32_collapse_issue_9509() {
        // range(0f0, 1f0, length = 3) — upstream collapses ref/step to plain
        // Float64 scalars for narrow-float element types.
        let r = RangeValue::float_linspace(0.0, 1.0, 3, RangeElementType::Float32);
        assert_eq!(
            r.julia_type_name(),
            "StepRangeLen{Float32, Float64, Float64, Int64}"
        );
        assert_eq!(r.to_vec(), vec![0.0, 0.5, 1.0]);
    }

    #[test]
    fn test_float_steplen_matches_upstream_issue_9509() {
        // range(0.0; step = 0.1, length = 4) — the floatrange rational path
        // keeps decimal steps exact (upstream r[4] == 0.3).
        let r = RangeValue::float_steplen(0.0, 0.1, 4, RangeElementType::Float64);
        assert_eq!(r.length(), 4);
        assert_eq!(r.get(4).unwrap(), 0.3);
        // Empty step-defined range: authoritative length 0, display endpoint
        // `r.ref + (len - offset) * step` (upstream `1.0:0.5:0.5` show form).
        let r = RangeValue::float_steplen(1.0, 0.5, 0, RangeElementType::Float64);
        assert_eq!(r.length(), 0);
        assert_eq!(r.stop, 0.5);
    }

    #[test]
    fn typed_step_uses_step_type_not_element_type_issue_9519() {
        let r = RangeValue {
            element_type: RangeElementType::Int16,
            step_type: RangeElementType::Int8,
            ..RangeValue::step_range(1.0, 2.0, 1.0)
        };

        assert!(matches!(r.typed_step(), super::super::Value::I8(2)));
        assert!(matches!(
            r.typed_element(r.start),
            super::super::Value::I16(1)
        ));
    }

    #[test]
    fn unit_range_typed_step_uses_element_type_issue_9811() {
        let r = RangeValue {
            element_type: RangeElementType::UInt16,
            step_type: RangeElementType::Default,
            ..RangeValue::unit_range(5.0, 1.0)
        };
        assert!(matches!(r.typed_step(), super::super::Value::U16(1)));

        let r = RangeValue {
            element_type: RangeElementType::BigInt,
            step_type: RangeElementType::Default,
            ..RangeValue::unit_range(1.0, 3.0)
        };
        assert!(matches!(r.typed_step(), super::super::Value::BigInt(_)));

        let r = RangeValue {
            element_type: RangeElementType::Char,
            step_type: RangeElementType::Default,
            ..RangeValue::unit_range(97.0, 99.0)
        };
        assert!(matches!(r.typed_step(), super::super::Value::I64(1)));
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
