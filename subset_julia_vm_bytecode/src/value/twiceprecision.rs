//! Twice-precision (double-double) arithmetic for float ranges.
//!
//! Port of the parts of upstream `julia/base/twiceprecision.jl` needed to
//! materialize TwicePrecision-backed float ranges: `0.1:0.1:0.3` must return
//! `r[3] == 0.3` (the shortest-decimal Float64), not the naive accumulation
//! `0.1 + 2*0.1 == 0.30000000000000004` (Issues #9419 / #9421).
//!
//! sjulia represents float colon ranges and `range(start, stop; length)` as
//! the VM-native [`super::RangeValue`]; this module supplies the upstream
//! construction (`floatrange`, `_linspace`) and indexing (`unsafe_getindex`)
//! algorithms those values use to compute their elements. The upstream
//! `typeof` form `StepRangeLen{Float64, Base.TwicePrecision{Float64},
//! Base.TwicePrecision{Float64}, Int64}` is already reported by the value
//! model (`value_enum.rs`).

// SAFETY: all f64 -> i64 casts below are guarded: `rat` bounds its state by
// `maxintfloat(Float32)` (2^24) before truncating, and index/rounding casts
// operate on values already clamped to a range length.
#![allow(clippy::cast_possible_truncation)]

/// `hi, lo = canonicalize2(big, little)` — upstream `canonicalize2`.
///
/// Renormalize so all nonzero bits in `hi` are more significant than any bit
/// in `lo`. `big` must be larger in magnitude than `little`.
#[inline]
pub(crate) fn canonicalize2(big: f64, little: f64) -> (f64, f64) {
    let h = big + little;
    (h, (big - h) + little)
}

/// `zhi, zlo = add12(x, y)` — exact double-double sum (upstream `add12`).
#[inline]
pub(crate) fn add12(x: f64, y: f64) -> (f64, f64) {
    let (x, y) = if y.abs() > x.abs() { (y, x) } else { (x, y) };
    canonicalize2(x, y)
}

/// `zhi, zlo = mul12(x, y)` — exact double-double product via FMA
/// (upstream `mul12` / `Math.two_mul`).
#[inline]
fn mul12(x: f64, y: f64) -> (f64, f64) {
    let h = x * y;
    if !h.is_finite() {
        return (h, h);
    }
    let l = x.mul_add(y, -h);
    (h, l)
}

/// Zero the low `nb` mantissa bits of `x` (upstream `truncbits`).
#[inline]
fn truncbits(x: f64, nb: u32) -> f64 {
    if nb == 0 {
        return x;
    }
    f64::from_bits(x.to_bits() & (u64::MAX << nb))
}

/// `cld(precision(Float64), 2)` — the split point used by `splitprec`.
const F64_HALF_PREC: u32 = 27;

/// Represent integer `i` as `(hi, lo)` with `hi + lo == i` exactly and `hi`
/// exactly multipliable by another `splitprec` `hi` (upstream `splitprec`).
#[inline]
fn splitprec(i: i128) -> (f64, f64) {
    let hi = truncbits(i as f64, F64_HALF_PREC);
    let ihi = hi as i128;
    (hi, (i - ihi) as f64)
}

/// Double-double number: `hi` carries the most significant bits, `lo` the
/// least significant (upstream `Base.TwicePrecision{Float64}`).
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct TwicePrecision {
    pub hi: f64,
    pub lo: f64,
}

impl TwicePrecision {
    /// `TwicePrecision{Float64}(x)` for an f64 (lo = 0).
    #[inline]
    pub fn from_f64(x: f64) -> Self {
        Self { hi: x, lo: 0.0 }
    }

    /// Raw `(hi, lo)` constructor (upstream `TwicePrecision{Float64}(hi, lo)`).
    #[inline]
    pub fn new(hi: f64, lo: f64) -> Self {
        Self { hi, lo }
    }

    /// `TwicePrecision{Float64}(i::Integer)` — exact for any `i64`, and for
    /// `i128` values up to ~2^106 (enough for `_linspace` numerators).
    #[inline]
    fn from_int(i: i128) -> Self {
        let (h, l) = splitprec(i);
        let (hi, lo) = canonicalize2(h, l);
        Self { hi, lo }
    }

    /// `TwicePrecision{Float64}((num, den))` — rational `num/den` to double-
    /// double precision.
    pub fn from_rational(num: i128, den: i128) -> Self {
        Self::from_int(num).div_f64(den as f64)
    }

    /// `TwicePrecision{Float64}((num, den), nb)` — rational constructor whose
    /// `hi` has `nb` trailing zero bits, so `(0:len-1) * hi` stays exact.
    pub fn from_rational_nb(num: i128, den: i128, nb: u32) -> Self {
        Self::from_rational(num, den).truncated(nb)
    }

    /// `twiceprecision(val, nb)` — move the low `nb` bits of `hi` into `lo`.
    #[inline]
    pub fn truncated(self, nb: u32) -> Self {
        let hi = truncbits(self.hi, nb);
        Self {
            hi,
            lo: (self.hi - hi) + self.lo,
        }
    }

    /// `x::TwicePrecision / v::Number` (upstream `/`).
    fn div_f64(self, v: f64) -> Self {
        self.div_tp(Self::from_f64(v))
    }

    /// `x::TwicePrecision / y::TwicePrecision` (upstream `/`).
    fn div_tp(self, y: Self) -> Self {
        let hi = self.hi / y.hi;
        let (uh, ul) = mul12(hi, y.hi);
        let lo = ((((self.hi - uh) - ul) + self.lo) - hi * y.lo) / y.hi;
        if hi == 0.0 || !hi.is_finite() {
            return Self { hi, lo: hi };
        }
        let (h, l) = canonicalize2(hi, lo);
        Self { hi: h, lo: l }
    }

    /// Collapse to a single `f64` (`Float64(x::TwicePrecision)`).
    #[inline]
    pub fn to_f64(self) -> f64 {
        self.hi + self.lo
    }
}

/// TwicePrecision-backed `StepRangeLen` parts: `r[i] = ref + (i - offset) *
/// step`, evaluated in double-double arithmetic.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct RangeHp {
    pub ref_: TwicePrecision,
    pub step: TwicePrecision,
    pub offset: i64,
}

impl RangeHp {
    /// Element at 1-based index `i` — upstream
    /// `unsafe_getindex(r::StepRangeLen{T,<:TwicePrecision,<:TwicePrecision}, i)`.
    /// Assumes `step.hi` has enough trailing zeros that `u * step.hi` is exact.
    #[inline]
    pub fn elem(&self, i: i64) -> f64 {
        let u = (i - self.offset) as f64;
        let shift_hi = u * self.step.hi;
        let shift_lo = u * self.step.lo;
        let (x_hi, x_lo) = add12(self.ref_.hi, shift_hi);
        x_hi + (x_lo + (shift_lo + self.ref_.lo))
    }

    /// The user-visible step, `T(r.step)` (upstream `step(::StepRangeLen)`).
    #[inline]
    pub fn step_f64(&self) -> f64 {
        self.step.to_f64()
    }
}

/// `nbitslen(len, offset)` — number of trailing zero bits needed in `step.hi`
/// so index shifts up to `max(offset-1, len-offset)` multiply exactly.
fn nbitslen_generic(len: i64, offset: i64) -> u32 {
    if len < 2 {
        return 0;
    }
    let k = (offset - 1).max(len - offset) - 1;
    // top_set_bit(k) + 1; k >= 0 here (offset in [1, len], len >= 2).
    let top = if k <= 0 {
        0
    } else {
        64 - k.unsigned_abs().leading_zeros()
    };
    top + 1
}

/// `nbitslen(Float64, len, offset)` — clamped to half the mantissa width.
fn nbitslen_f64(len: i64, offset: i64) -> u32 {
    F64_HALF_PREC.min(nbitslen_generic(len, offset))
}

/// `cld(precision(Float32), 2)` — the Float32 analogue of `F64_HALF_PREC`.
const F32_HALF_PREC: u32 = 12;

/// `nbitslen(Float32, len, offset)` — clamped to half the Float32 mantissa
/// width (upstream `nbitslen(T, len, offset)` with `T == Float32`).
fn nbitslen_f32(len: i64, offset: i64) -> u32 {
    F32_HALF_PREC.min(nbitslen_generic(len, offset))
}

/// `canonicalize2` in the range's own (Float32) precision.
#[inline]
fn canonicalize2_f32(big: f32, little: f32) -> (f32, f32) {
    let h = big + little;
    (h, (big - h) + little)
}

/// `add12` in Float32 arithmetic (upstream `add12(x::T, y::T)` with
/// `T == Float32`).
#[inline]
fn add12_f32(x: f32, y: f32) -> (f32, f32) {
    let (x, y) = if y.abs() > x.abs() { (y, x) } else { (x, y) };
    canonicalize2_f32(x, y)
}

/// `truncbits` on a Float32 mantissa.
#[inline]
fn truncbits_f32(x: f32, nb: u32) -> f32 {
    if nb == 0 {
        return x;
    }
    f32::from_bits(x.to_bits() & (u32::MAX << nb))
}

/// `maxintfloat(Float32)` — the `rat` state bound (upstream uses
/// `maxintfloat(narrow(Float64), Int)` with `narrow(Float64) == Float32`).
const RAT_BOUND_F64: f64 = 16_777_216.0; // 2^24

/// `maxintfloat(Float16)` — the `rat` state bound for Float32 inputs.
const RAT_BOUND_F32: f64 = 2_048.0; // 2^11

/// `maxintfloat(Float64, Int)` — "will rounding to integer succeed" bound.
const MAXINTFLOAT_F64: f64 = 9_007_199_254_740_992.0; // 2^53

/// `maxintfloat(Float32, Int)` — same bound for Float32 range endpoints.
const MAXINTFLOAT_F32: f64 = 16_777_216.0; // 2^24

/// Approximate `x` by a rational `num/den` pair via continued fractions
/// (upstream `rat`). Guaranteed to return, not guaranteed to be exact; the
/// caller must verify `num/den` rounds back to `x`.
fn rat(x: f64, bound: f64) -> (i64, i64) {
    let mut y = x;
    let (mut a, mut d) = (1i64, 1i64);
    let (mut b, mut c) = (0i64, 0i64);
    while y.abs() <= bound {
        let f = y.trunc() as i64;
        y -= f as f64;
        let (na, nc) = (f * a + c, a);
        a = na;
        c = nc;
        let (nb, nd) = (f * b + d, b);
        b = nb;
        d = nd;
        if (a.abs().max(b.abs())) as f64 > bound {
            return (c, d);
        }
        if (a as f64) / (b as f64) == x {
            break;
        }
        y = 1.0 / y;
    }
    (a, b)
}

/// `rat` over Float64 values (colon / linspace on Float64 ranges).
pub(crate) fn rat_f64(x: f64) -> (i64, i64) {
    rat(x, RAT_BOUND_F64)
}

/// `rat` over (exactly widened) Float32 values, using Float32 arithmetic —
/// upstream runs the continued fraction in `T == Float32`.
fn rat_f32(x: f32) -> (i64, i64) {
    let mut y = x;
    let (mut a, mut d) = (1i64, 1i64);
    let (mut b, mut c) = (0i64, 0i64);
    while f64::from(y.abs()) <= RAT_BOUND_F32 {
        let f = y.trunc() as i64;
        y -= f as f32;
        let (na, nc) = (f * a + c, a);
        a = na;
        c = nc;
        let (nb, nd) = (f * b + d, b);
        b = nb;
        d = nd;
        if (a.abs().max(b.abs())) as f64 > RAT_BOUND_F32 {
            return (c, d);
        }
        if (a as f32) / (b as f32) == x {
            break;
        }
        y = 1.0 / y;
    }
    (a, b)
}

/// `lcm_unchecked(a, b)` — lcm without overflow checking (upstream).
fn lcm_unchecked(a: i64, b: i64) -> i64 {
    let g = gcd(a, b);
    if g == 0 {
        return 0;
    }
    a.wrapping_mul(b / g)
}

fn gcd(mut a: i64, mut b: i64) -> i64 {
    while b != 0 {
        let t = b;
        b = a % b;
        a = t;
    }
    a.abs()
}

/// `isbetween(a, x, b)` (upstream).
fn isbetween(a: f64, x: f64, b: f64) -> bool {
    (a <= x && x <= b) || (b <= x && x <= a)
}

/// Whether the hp machinery treats the range as Float64-backed (TwicePrecision
/// ref/step) or Float32-backed (plain Float64 ref/step, upstream
/// `StepRangeLen{Float32, Float64, Float64, Int64}`).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum HpElement {
    F64,
    F32,
}

/// `floatrange(T, start_n, step_n, len, den)` — range for rational
/// `start = start_n/den`, `step = step_n/den` (upstream `floatrange`).
fn floatrange(t: HpElement, start_n: i64, step_n: i64, len: i64, den: i64) -> RangeHp {
    if len < 2 || step_n == 0 {
        return steprangelen_hp_rational(
            t,
            i128::from(start_n),
            i128::from(step_n),
            den.into(),
            0,
            1,
        );
    }
    // Index of the smallest-magnitude value.
    let imin = (-(start_n as f64) / (step_n as f64) + 1.0).round_ties_even();
    let imin = if imin.is_finite() {
        (imin as i64).clamp(1, len)
    } else {
        1
    };
    // Smallest-magnitude element to 2x precision; can't overflow (|start_n|,
    // |step_n| <= 2^53 checked by the caller, len bounded by them).
    let ref_n = i128::from(start_n) + i128::from(imin - 1) * i128::from(step_n);
    let nb = nbitslen_f64(len, imin);
    steprangelen_hp_rational(t, ref_n, i128::from(step_n), den.into(), nb, imin)
}

/// `steprangelen_hp(T, (ref_n, den), (step_n, den), nb, len, offset)` for the
/// two supported element kinds.
fn steprangelen_hp_rational(
    t: HpElement,
    ref_n: i128,
    step_n: i128,
    den: i128,
    nb: u32,
    offset: i64,
) -> RangeHp {
    match t {
        HpElement::F64 => RangeHp {
            ref_: TwicePrecision::from_rational(ref_n, den),
            step: TwicePrecision::from_rational_nb(step_n, den, nb),
            offset,
        },
        // Float32 ranges store plain Float64 ref/step (upstream
        // `steprangelen_hp(::Type{T<:IEEEFloat}, ...)` = `ref[1]/ref[2]`).
        HpElement::F32 => RangeHp {
            ref_: TwicePrecision::from_f64(ref_n as f64 / den as f64),
            step: TwicePrecision::from_f64(step_n as f64 / den as f64),
            offset,
        },
    }
}

/// Literal (non-rational) `steprangelen_hp(T, start, step, 0, len, 1)`.
fn steprangelen_hp_literal(start: f64, step: f64) -> RangeHp {
    RangeHp {
        ref_: TwicePrecision::from_f64(start),
        step: TwicePrecision::from_f64(step),
        offset: 1,
    }
}

/// TwicePrecision parts for the float colon range `start:step:stop` with
/// (already computed) length `len` — the rational branch of upstream
/// `(:)(start::T, step::T, stop::T) where T<:IEEEFloat`, falling back to the
/// literal representation when the endpoints have no exact small rational.
///
/// `len` comes from the sjulia range-length model (`RangeValue::length`); the
/// rational branch is only taken when upstream's own length computation
/// agrees, so the values match upstream exactly whenever the lengths do.
pub fn colon_hp(t: HpElement, start: f64, step: f64, stop: f64, len: i64) -> RangeHp {
    if let Some(hp) = colon_hp_rational(t, start, step, stop, len) {
        return hp;
    }
    steprangelen_hp_literal(start, step)
}

#[derive(Debug, Clone, Copy)]
struct ColonRational {
    len: i64,
    start_n: i64,
    step_n: i64,
    den: i64,
}

/// Upstream rational length for `start:step:stop` when the IEEEFloat colon
/// endpoints have an exact small-rational representation.
pub fn colon_hp_length(t: HpElement, start: f64, step: f64, stop: f64) -> Option<i64> {
    colon_rational(t, start, step, stop).map(|r| r.len)
}

fn colon_hp_rational(t: HpElement, start: f64, step: f64, stop: f64, len: i64) -> Option<RangeHp> {
    let rational = colon_rational(t, start, step, stop)?;
    // Only use the rational path when upstream's length matches the length the
    // rest of the VM derives for this range, keeping len/getindex consistent.
    if rational.len != len {
        return None;
    }
    Some(floatrange(
        t,
        rational.start_n,
        rational.step_n,
        rational.len,
        rational.den,
    ))
}

fn colon_rational(t: HpElement, start: f64, step: f64, stop: f64) -> Option<ColonRational> {
    if step == 0.0 {
        return None;
    }
    let ((step_n, step_d), (start_n0, start_d), (stop_n, stop_d), m) = match t {
        HpElement::F64 => (
            rat_f64(step),
            rat_f64(start),
            rat_f64(stop),
            MAXINTFLOAT_F64,
        ),
        HpElement::F32 => (
            rat_f32(step as f32),
            rat_f32(start as f32),
            rat_f32(stop as f32),
            MAXINTFLOAT_F32,
        ),
    };
    // Exactness checks (in the range's own precision, like upstream).
    let exact = |n: i64, d: i64, x: f64| -> bool {
        if d == 0 {
            return false;
        }
        match t {
            HpElement::F64 => (n as f64) / (d as f64) == x,
            HpElement::F32 => (n as f32) / (d as f32) == x as f32,
        }
    };
    if !exact(step_n, step_d, step)
        || !exact(start_n0, start_d, start)
        || !exact(stop_n, stop_d, stop)
    {
        return None;
    }
    // Use the same denominator for start and step.
    let den = lcm_unchecked(start_d, step_d);
    if den == 0
        || (start * den as f64).abs() > m
        || (step * den as f64).abs() > m
        || den % start_d != 0
        || den % step_d != 0
    {
        return None;
    }
    let start_n = (start * den as f64).round_ties_even() as i64;
    let step_n = (step * den as f64).round_ties_even() as i64;
    // Upstream's length; computed in i128 so the integer ops cannot overflow.
    let upstream_len = {
        let num = i128::from(den) * i128::from(stop_n) - i128::from(stop_d) * i128::from(start_n)
            + i128::from(step_n) * i128::from(stop_d);
        let denom = i128::from(step_n) * i128::from(stop_d);
        if denom == 0 {
            return None;
        }
        (num / denom).max(0)
    };
    // Sanity checks, mirroring upstream's isbetween guards.
    let ulen = i64::try_from(upstream_len).ok()?;
    let len_ok = isbetween(start, start + (ulen - 1) as f64 * step, stop + step / 2.0)
        && !isbetween(start, start + ulen as f64 * step, stop);
    if !len_ok {
        return None;
    }
    Some(ColonRational {
        len: ulen,
        start_n,
        step_n,
        den,
    })
}

/// TwicePrecision parts for `range(start, stop; length = len)` — upstream
/// `range_start_stop_length(start::T, stop::T, len) where T<:IEEEFloat`
/// (twiceprecision.jl:645) for the two supported element kinds.
/// `HpElement::F32` runs the rational search in the range's own precision and
/// collapses ref/step to plain Float64 scalars (upstream
/// `steprangelen_hp(::Type{T}, ...) where T<:IEEEFloat` for `T != Float64`),
/// producing the `StepRangeLen{Float32/Float16, Float64, Float64, Int64}`
/// value model (Issue #9509).
pub fn linspace_hp(t: HpElement, start: f64, stop: f64, len: i64) -> RangeHp {
    match t {
        HpElement::F64 => linspace_hp_f64(start, stop, len),
        HpElement::F32 => linspace_hp_f32(start as f32, stop as f32, len),
    }
}

/// TwicePrecision parts for `range(start, stop; length = len)` on Float64
/// endpoints — upstream `range_start_stop_length(start::T, stop::T, len)
/// where T<:IEEEFloat` + `_linspace` (twiceprecision.jl).
///
/// Only called with `len >= 2` and finite endpoints; `len < 2` cases are
/// handled by `linspace1_hp`.
pub fn linspace_hp_f64(start: f64, stop: f64, len: i64) -> RangeHp {
    debug_assert!(len >= 2);
    if start == stop {
        return steprangelen_hp_literal(start, 0.0);
    }
    // Attempt exact rational approximations of both endpoints.
    let (_, start_d) = rat_f64(start);
    let (_, stop_d) = rat_f64(stop);
    if start_d != 0 && stop_d != 0 {
        let den = lcm_unchecked(start_d, stop_d);
        if den != 0
            && (den as f64 * start).abs() <= MAXINTFLOAT_F64
            && (den as f64 * stop).abs() <= MAXINTFLOAT_F64
        {
            let start_n = (den as f64 * start).round_ties_even() as i64;
            let stop_n = (den as f64 * stop).round_ties_even() as i64;
            if (start_n as f64) / (den as f64) == start && (stop_n as f64) / (den as f64) == stop {
                return linspace_rational_f64(start_n, stop_n, len, den);
            }
        }
    }
    linspace_general_f64(start, stop, len)
}

/// `_linspace(Float64, start_n, stop_n, len, den)` — rational endpoints.
fn linspace_rational_f64(start_n: i64, stop_n: i64, len: i64, den: i64) -> RangeHp {
    debug_assert!(len >= 2);
    if start_n == stop_n {
        return steprangelen_hp_rational(HpElement::F64, start_n.into(), 0, den.into(), 0, 1);
    }
    let tmin = -(start_n as f64) / (stop_n as f64 - start_n as f64);
    let imin = (tmin * (len - 1) as f64 + 1.0).round_ties_even();
    let imin = if imin.is_finite() {
        (imin as i64).clamp(1, len)
    } else {
        1
    };
    // Widened arithmetic, like upstream's `W = widen(L)`.
    let ref_num =
        i128::from(len - imin) * i128::from(start_n) + i128::from(imin - 1) * i128::from(stop_n);
    let ref_denom = i128::from(len - 1) * i128::from(den);
    let step_num = i128::from(stop_n) - i128::from(start_n);
    let nb = nbitslen_f64(len, imin);
    RangeHp {
        ref_: TwicePrecision::from_rational(ref_num, ref_denom),
        step: TwicePrecision::from_rational_nb(step_num, ref_denom, nb),
        offset: imin,
    }
}

/// `_linspace(start::Float64, stop::Float64, len)` — general endpoints with
/// high-precision endpoint matching (first(r) == start, last(r) == stop).
fn linspace_general_f64(start: f64, stop: f64, len: i64) -> RangeHp {
    debug_assert!(len >= 2);
    // Find the index that returns the smallest-magnitude element.
    let (mut delta, mut delta_fac) = (stop - start, 1.0f64);
    if !delta.is_finite() {
        // Handle overflow for large endpoints.
        delta = stop / len as f64 - start / len as f64;
        delta_fac = len as f64;
    }
    let tmin = -(start / delta) / delta_fac; // t such that (1-t)*start + t*stop == 0
    let lenn1 = (len - 1) as f64;
    let imin_f = (tmin * lenn1 + 1.0).round_ties_even();
    let imin_raw = if imin_f.is_finite() { imin_f as i64 } else { 1 };
    let (imin, ref_, step) = if 1 < imin_raw && imin_raw < len {
        // The smallest-magnitude element is in the interior.
        let t = (imin_raw - 1) as f64 / lenn1;
        let r = (1.0 - t) * start + t * stop;
        let s = if imin_raw - 1 < len - imin_raw {
            (r - start) / (imin_raw - 1) as f64
        } else {
            (stop - r) / (len - imin_raw) as f64
        };
        (imin_raw, r, s)
    } else if imin_raw <= 1 {
        (1, start, (delta / lenn1) * delta_fac)
    } else {
        (len, stop, (delta / lenn1) * delta_fac)
    };
    if len == 2 && !step.is_finite() {
        // For very large endpoints where step overflows, exploit the split
        // representation to handle the overflow.
        return RangeHp {
            ref_: TwicePrecision::from_f64(start),
            step: TwicePrecision::new(-start, stop),
            offset: 1,
        };
    }
    // 2x calculations to get high-precision endpoint matching while also
    // preventing overflow in ref_hi + (i - offset) * step_hi.
    let m = f64::from_bits(f64::MAX.to_bits() - 1); // prevfloat(floatmax(T))
    let k = ((imin - 1).max(len - imin)) as f64;
    let lo_bound = (-(m + ref_) / k).max((-m + ref_) / k);
    let hi_bound = ((m - ref_) / k).min((m + ref_) / k);
    let step_hi_pre = step.clamp(lo_bound, hi_bound);
    let nb = nbitslen_f64(len, imin);
    let step_hi = truncbits(step_hi_pre, nb);
    let (x1_hi, x1_lo) = add12((1 - imin) as f64 * step_hi, ref_);
    let (x2_hi, x2_lo) = add12((len - imin) as f64 * step_hi, ref_);
    let a = (start - x1_hi) - x1_lo;
    let b = (stop - x2_hi) - x2_lo;
    let step_lo = (b - a) / lenn1;
    let ref_lo = a - (1 - imin) as f64 * step_lo;
    RangeHp {
        ref_: TwicePrecision::new(ref_, ref_lo),
        step: TwicePrecision::new(step_hi, step_lo),
        offset: imin,
    }
}

/// `range_start_stop_length(start::Float32, stop::Float32, len)` — the same
/// algorithm as the Float64 path, run in the range's own precision with the
/// `steprangelen_hp(::Type{T != Float64}, ...)` plain-Float64 collapse.
fn linspace_hp_f32(start: f32, stop: f32, len: i64) -> RangeHp {
    debug_assert!(len >= 2);
    if start == stop {
        // Upstream: steprangelen_hp(T, start, zero(T), 0, len, 1).
        return steprangelen_hp_literal(f64::from(start), 0.0);
    }
    // Attempt exact rational approximations of both endpoints (in Float32
    // arithmetic, like upstream's `rat(start)` / `abs(den*start) <= m` with
    // `m = maxintfloat(T, Int)`).
    let (_, start_d) = rat_f32(start);
    let (_, stop_d) = rat_f32(stop);
    if start_d != 0 && stop_d != 0 {
        let den = lcm_unchecked(start_d, stop_d);
        if den != 0
            && f64::from((den as f32 * start).abs()) <= MAXINTFLOAT_F32
            && f64::from((den as f32 * stop).abs()) <= MAXINTFLOAT_F32
        {
            let start_n = (den as f32 * start).round_ties_even() as i64;
            let stop_n = (den as f32 * stop).round_ties_even() as i64;
            // Upstream `T(start_n/den) == start`: Int/Int division in Float64,
            // converted to T for the comparison.
            if (start_n as f64 / den as f64) as f32 == start
                && (stop_n as f64 / den as f64) as f32 == stop
            {
                return linspace_rational_f32(start_n, stop_n, len, den);
            }
        }
    }
    linspace_general_f32(start, stop, len)
}

/// `_linspace(Float32, start_n, stop_n, len, den)` — rational endpoints with
/// the `T != Float64` collapse: `steprangelen_hp(T, ref::Tuple, step::Tuple,
/// nb, len, offset)` is the plain division `ref[1]/ref[2]` / `step[1]/step[2]`
/// (nb is ignored for the non-TwicePrecision representation).
fn linspace_rational_f32(start_n: i64, stop_n: i64, len: i64, den: i64) -> RangeHp {
    debug_assert!(len >= 2);
    if start_n == stop_n {
        return RangeHp {
            ref_: TwicePrecision::from_f64(start_n as f64 / den as f64),
            step: TwicePrecision::from_f64(0.0),
            offset: 1,
        };
    }
    let tmin = -(start_n as f64) / (stop_n as f64 - start_n as f64);
    let imin = (tmin * (len - 1) as f64 + 1.0).round_ties_even();
    let imin = if imin.is_finite() {
        (imin as i64).clamp(1, len)
    } else {
        1
    };
    // Widened arithmetic, like upstream's `W = widen(L)`.
    let ref_num =
        i128::from(len - imin) * i128::from(start_n) + i128::from(imin - 1) * i128::from(stop_n);
    let ref_denom = i128::from(len - 1) * i128::from(den);
    let step_num = i128::from(stop_n) - i128::from(start_n);
    RangeHp {
        ref_: TwicePrecision::from_f64(ref_num as f64 / ref_denom as f64),
        step: TwicePrecision::from_f64(step_num as f64 / ref_denom as f64),
        offset: imin,
    }
}

/// `_linspace(start::Float32, stop::Float32, len)` — general endpoints. All
/// arithmetic runs in the range's own precision (upstream `_linspace(start::T,
/// stop::T, len) where T<:IEEEFloat` with `T == Float32`), and the final
/// `steprangelen_hp(T, (ref, ref_lo), (step_hi, step_lo), ...)` collapse is
/// `asF64(ref)` / `asF64(step)`.
fn linspace_general_f32(start: f32, stop: f32, len: i64) -> RangeHp {
    debug_assert!(len >= 2);
    // Find the index that returns the smallest-magnitude element.
    let (mut delta, mut delta_fac) = (stop - start, 1.0f32);
    if !delta.is_finite() {
        // Handle overflow for large endpoints.
        delta = stop / len as f32 - start / len as f32;
        delta_fac = len as f32;
    }
    let tmin = -(start / delta) / delta_fac; // t such that (1-t)*start + t*stop == 0
    let lenn1_f = (len - 1) as f32;
    let imin_f = (tmin * lenn1_f + 1.0).round_ties_even();
    let imin_raw = if imin_f.is_finite() { imin_f as i64 } else { 1 };
    let (imin, ref_, step) = if 1 < imin_raw && imin_raw < len {
        // The smallest-magnitude element is in the interior. Upstream computes
        // `t = (imin - 1)/lenn1` (Int/Int -> Float64) and converts the lerp
        // back to T.
        let t = (imin_raw - 1) as f64 / (len - 1) as f64;
        let r = (((1.0 - t) * f64::from(start)) + t * f64::from(stop)) as f32;
        let s = if imin_raw - 1 < len - imin_raw {
            (r - start) / (imin_raw - 1) as f32
        } else {
            (stop - r) / (len - imin_raw) as f32
        };
        (imin_raw, r, s)
    } else if imin_raw <= 1 {
        (1, start, (delta / lenn1_f) * delta_fac)
    } else {
        (len, stop, (delta / lenn1_f) * delta_fac)
    };
    if len == 2 && !step.is_finite() {
        // For very large endpoints where step overflows, exploit the split
        // representation: steprangelen_hp(T, start, (-start, stop), ...) with
        // the asF64 collapse `Float64(-start) + Float64(stop)`.
        return RangeHp {
            ref_: TwicePrecision::from_f64(f64::from(start)),
            step: TwicePrecision::from_f64(f64::from(-start) + f64::from(stop)),
            offset: 1,
        };
    }
    // 2x calculations to get high-precision endpoint matching while also
    // preventing overflow in ref_hi + (i - offset) * step_hi.
    let m = f32::from_bits(f32::MAX.to_bits() - 1); // prevfloat(floatmax(T))
    let k = ((imin - 1).max(len - imin)) as f32;
    let lo_bound = (-(m + ref_) / k).max((-m + ref_) / k);
    let hi_bound = ((m - ref_) / k).min((m + ref_) / k);
    // Julia `clamp` (min/max chain — never panics on inverted bounds).
    let step_hi_pre = step.max(lo_bound).min(hi_bound);
    let nb = nbitslen_f32(len, imin);
    let step_hi = truncbits_f32(step_hi_pre, nb);
    let (x1_hi, x1_lo) = add12_f32((1 - imin) as f32 * step_hi, ref_);
    let (x2_hi, x2_lo) = add12_f32((len - imin) as f32 * step_hi, ref_);
    let a = (start - x1_hi) - x1_lo;
    let b = (stop - x2_hi) - x2_lo;
    let step_lo = (b - a) / lenn1_f;
    let ref_lo = a - (1 - imin) as f32 * step_lo;
    RangeHp {
        ref_: TwicePrecision::from_f64(f64::from(ref_) + f64::from(ref_lo)),
        step: TwicePrecision::from_f64(f64::from(step_hi) + f64::from(step_lo)),
        offset: imin,
    }
}

/// `_linspace1(T, start, stop, len)` — the `len < 2` cases of
/// `range(start, stop; length)` for the two supported element kinds. The
/// caller must have validated `len >= 0` and (for `len == 1`)
/// `start == stop`.
pub fn linspace1_hp(t: HpElement, start: f64, stop: f64, len: i64) -> RangeHp {
    match t {
        HpElement::F64 => linspace1_hp_f64(start, stop, len),
        HpElement::F32 => {
            debug_assert!((0..2).contains(&len));
            // Upstream `_linspace1(::Type{T<:Union{Float32,Float16}}, ...)`:
            // StepRangeLen{T}(Float64(start), Float64(start) - Float64(stop), len, 1).
            let s = f64::from(start as f32);
            let e = f64::from(stop as f32);
            RangeHp {
                ref_: TwicePrecision::from_f64(s),
                step: TwicePrecision::from_f64(s - e),
                offset: 1,
            }
        }
    }
}

/// `_linspace1(Float64, start, stop, len)` — the `len < 2` cases of
/// `range(start, stop; length)`. The caller must have validated `len >= 0`
/// and (for `len == 1`) `start == stop`.
pub fn linspace1_hp_f64(start: f64, stop: f64, len: i64) -> RangeHp {
    debug_assert!((0..2).contains(&len));
    // Ensure first(r) == start and last(r) == stop even for len == 0.
    RangeHp {
        ref_: TwicePrecision::from_f64(start),
        step: TwicePrecision::new(start, -stop),
        offset: 1,
    }
}

/// TwicePrecision parts for `range(start; step, length = len)` — upstream
/// `range_start_step_length(a::T, st::T, len::Integer) where T<:IEEEFloat`
/// (twiceprecision.jl:448, Issue #9509). The length is authoritative;
/// elements follow `floatrange` when start/step have exact small rationals,
/// falling back to the literal representation otherwise.
pub fn steprangelen_hp_from_step(t: HpElement, start: f64, step: f64, len: i64) -> RangeHp {
    match t {
        HpElement::F64 => {
            let (start_n0, start_d) = rat_f64(start);
            let (step_n0, step_d) = rat_f64(step);
            if start_d != 0
                && step_d != 0
                && (start_n0 as f64) / (start_d as f64) == start
                && (step_n0 as f64) / (step_d as f64) == step
            {
                let den = lcm_unchecked(start_d, step_d);
                if den != 0
                    && (den as f64 * start).abs() <= MAXINTFLOAT_F64
                    && (den as f64 * step).abs() <= MAXINTFLOAT_F64
                    && den % start_d == 0
                    && den % step_d == 0
                {
                    let start_n = (den as f64 * start).round_ties_even() as i64;
                    let step_n = (den as f64 * step).round_ties_even() as i64;
                    return floatrange(t, start_n, step_n, len, den);
                }
            }
            steprangelen_hp_literal(start, step)
        }
        HpElement::F32 => {
            let start32 = start as f32;
            let step32 = step as f32;
            let (start_n0, start_d) = rat_f32(start32);
            let (step_n0, step_d) = rat_f32(step32);
            if start_d != 0
                && step_d != 0
                && (start_n0 as f64 / start_d as f64) as f32 == start32
                && (step_n0 as f64 / step_d as f64) as f32 == step32
            {
                let den = lcm_unchecked(start_d, step_d);
                if den != 0
                    && f64::from((den as f32 * start32).abs()) <= MAXINTFLOAT_F32
                    && f64::from((den as f32 * step32).abs()) <= MAXINTFLOAT_F32
                    && den % start_d == 0
                    && den % step_d == 0
                {
                    let start_n = (den as f32 * start32).round_ties_even() as i64;
                    let step_n = (den as f32 * step32).round_ties_even() as i64;
                    return floatrange(t, start_n, step_n, len, den);
                }
            }
            steprangelen_hp_literal(f64::from(start32), f64::from(step32))
        }
    }
}

// ---------------------------------------------------------------------------
// Complex{Float64} TwicePrecision — scaled ranges (Issue #9659)
// ---------------------------------------------------------------------------
// Port of the upstream algebra as it executes for
// `x::Complex .* r::StepRangeLen{Float64, TwicePrecision, TwicePrecision}`
// (julia/base/broadcast.jl:1169 → twiceprecision.jl). `ComplexF64` is not an
// `IEEEFloat`, so `mul12` takes the generic `(p = x*y; (p, zero(p)))` arm and
// `add12`/`canonicalize2` operate on whole complex values (the `add12` swap
// compares complex magnitudes, i.e. `hypot`). Every operation below mirrors
// its upstream counterpart verbatim — including signed-zero behavior — so the
// scaled range's elements are bit-identical to upstream's lazy
// `StepRangeLen{ComplexF64, TwicePrecision{ComplexF64}, …}`.

/// A `ComplexF64` value with julia-exact arithmetic helpers.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct C64 {
    pub re: f64,
    pub im: f64,
}

impl C64 {
    #[inline]
    pub fn new(re: f64, im: f64) -> Self {
        Self { re, im }
    }

    #[inline]
    fn zero() -> Self {
        Self { re: 0.0, im: 0.0 }
    }

    /// julia `iszero(z)` — both parts zero (either sign).
    #[inline]
    fn is_zero(self) -> bool {
        self.re == 0.0 && self.im == 0.0
    }

    /// julia `isfinite(z)` — both parts finite.
    #[inline]
    fn is_finite(self) -> bool {
        self.re.is_finite() && self.im.is_finite()
    }

    /// julia `abs(z) = hypot(real(z), imag(z))`.
    #[inline]
    fn abs(self) -> f64 {
        julia_hypot(self.re, self.im)
    }
}

/// julia `+(z::Complex, w::Complex)` — per-part.
#[inline]
fn cadd(a: C64, b: C64) -> C64 {
    C64::new(a.re + b.re, a.im + b.im)
}

/// julia `-(z::Complex, w::Complex)` — per-part.
#[inline]
fn csub(a: C64, b: C64) -> C64 {
    C64::new(a.re - b.re, a.im - b.im)
}

/// julia `*(z::Complex, w::Complex)` — the exact naive formula (same op
/// order as upstream `complex.jl`, so signed zeros match).
#[inline]
fn cmul(a: C64, b: C64) -> C64 {
    C64::new(a.re * b.re - a.im * b.im, a.re * b.im + a.im * b.re)
}

/// julia `*(x::Real, z::Complex) = Complex(x*real(z), x*imag(z))`.
#[inline]
fn cmul_real(x: f64, z: C64) -> C64 {
    C64::new(x * z.re, x * z.im)
}

/// Port of upstream `Base.Math._hypot(x::Float64, y::Float64)` with the
/// native-FMA branch (julia/base/math.jl). Used only for the `add12` operand
/// ordering, but ported faithfully so a near-tie magnitude comparison cannot
/// diverge from upstream.
fn julia_hypot(x: f64, y: f64) -> f64 {
    let mut ax = x.abs();
    let mut ay = y.abs();
    if ax.is_infinite() || ay.is_infinite() {
        return f64::INFINITY;
    }
    if ay > ax {
        core::mem::swap(&mut ax, &mut ay);
    }
    // Widely varying operands (also catches ay == 0): sqrt(eps(Float64)/2).
    if ay <= ax * (f64::EPSILON / 2.0).sqrt() {
        return ax;
    }
    // Rescaling constant: eps(Float64) * sqrt(floatmin(Float64)).
    let mut scale = f64::EPSILON * f64::MIN_POSITIVE.sqrt();
    if ax > (f64::MAX / 2.0).sqrt() {
        ax *= scale;
        ay *= scale;
        scale = 1.0 / scale;
    } else if ay < f64::MIN_POSITIVE.sqrt() {
        ax /= scale;
        ay /= scale;
    } else {
        scale = 1.0;
    }
    let mut h = ax.mul_add(ax, ay * ay).sqrt();
    // Correctly-rounded correction (requires native fma; aarch64/x86-64 have it).
    let hsquared = h * h;
    let axsquared = ax * ax;
    h -= ((-ay).mul_add(ay, hsquared - axsquared) + h.mul_add(h, -hsquared)
        - ax.mul_add(ax, -axsquared))
        / (2.0 * h);
    h * scale
}

/// `canonicalize2` on complex values (the generic upstream method: no swap).
#[inline]
fn canonicalize2_c(big: C64, little: C64) -> (C64, C64) {
    let h = cadd(big, little);
    (h, cadd(csub(big, h), little))
}

/// `add12` on complex values — swap by magnitude, then `canonicalize2`.
#[inline]
fn add12_c(x: C64, y: C64) -> (C64, C64) {
    let (x, y) = if y.abs() > x.abs() { (y, x) } else { (x, y) };
    canonicalize2_c(x, y)
}

/// Double-double `ComplexF64` (upstream `TwicePrecision{ComplexF64}`).
#[derive(Debug, Clone, Copy)]
pub struct TwicePrecisionC {
    pub hi: C64,
    pub lo: C64,
}

/// Upstream `*(x::TwicePrecision{Float64}, v::Complex)`:
/// `v == 0` short-circuit, else the full
/// `*(::TwicePrecision{ComplexF64}, ::TwicePrecision{ComplexF64})` against
/// `TwicePrecision(v)` after promotion. `mul12` is the generic non-IEEEFloat
/// arm `(p = x*y; (p, zero(p)))`.
fn tp_scale_complex(x: TwicePrecision, v: C64) -> TwicePrecisionC {
    if v.is_zero() {
        return TwicePrecisionC {
            hi: cmul_real(x.hi, v),
            lo: cmul_real(x.lo, v),
        };
    }
    // Promote: x -> TwicePrecision{ComplexF64}((x.hi, 0), (x.lo, 0));
    // y = TwicePrecision(v) = (v, 0).
    let x_hi = C64::new(x.hi, 0.0);
    let x_lo = C64::new(x.lo, 0.0);
    let y_hi = v;
    let y_lo = C64::zero();
    // zh, zl = mul12(x.hi, y.hi) — generic arm.
    let zh = cmul(x_hi, y_hi);
    let zl = C64::zero();
    // ret = TwicePrecision(canonicalize2(zh, (x.hi*y.lo + x.lo*y.hi) + zl)...)
    let s = cadd(cadd(cmul(x_hi, y_lo), cmul(x_lo, y_hi)), zl);
    let (hi, lo) = canonicalize2_c(zh, s);
    if zh.is_zero() || !zh.is_finite() {
        TwicePrecisionC { hi: zh, lo: zh }
    } else {
        TwicePrecisionC { hi, lo }
    }
}

/// A complex-scaled TwicePrecision range: upstream
/// `broadcasted(*, x::Number, r::StepRangeLen{T}) =
///  StepRangeLen{typeof(x*T(r.ref))}(x*r.ref, x*r.step, length(r), r.offset)`
/// with a `Complex` scalar `x` (julia/base/broadcast.jl:1169).
#[derive(Debug, Clone, Copy)]
pub struct RangeHpComplex {
    pub ref_: TwicePrecisionC,
    pub step: TwicePrecisionC,
    pub offset: i64,
}

impl RangeHp {
    /// Scale by a complex scalar (Issue #9659).
    pub fn scale_complex(&self, x: C64) -> RangeHpComplex {
        RangeHpComplex {
            ref_: tp_scale_complex(self.ref_, x),
            step: tp_scale_complex(self.step, x),
            offset: self.offset,
        }
    }
}

impl RangeHpComplex {
    /// Element at 1-based index `i` — upstream `unsafe_getindex` for
    /// `StepRangeLen{ComplexF64, TwicePrecision{ComplexF64}, …}`.
    #[inline]
    pub fn elem(&self, i: i64) -> C64 {
        // u = oftype(r.offset, i) - r.offset; Int64 * ComplexF64 multiplies
        // per part after exact f64 conversion (|u| < 2^53).
        let u = (i - self.offset) as f64;
        let shift_hi = cmul_real(u, self.step.hi);
        let shift_lo = cmul_real(u, self.step.lo);
        let (x_hi, x_lo) = add12_c(self.ref_.hi, shift_hi);
        // T(x_hi + (x_lo + (shift_lo + r.ref.lo)))
        cadd(x_hi, cadd(x_lo, cadd(shift_lo, self.ref_.lo)))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    // ── TwicePrecision construction ───────────────────────────────────────────

    #[test]
    fn from_rational_one_tenth_matches_upstream() {
        // julia> Base.TwicePrecision{Float64}((1, 10))
        //   hi = 0.1, lo = -5.551115123125783e-18
        let tp = TwicePrecision::from_rational(1, 10);
        assert_eq!(tp.hi, 0.1);
        assert_eq!(tp.lo, -5.551115123125783e-18);
    }

    #[test]
    fn truncated_moves_low_bits_to_lo() {
        let tp = TwicePrecision::from_f64(0.1).truncated(10);
        assert_eq!(tp.hi + tp.lo, 0.1);
        assert_eq!(tp.hi.to_bits() & ((1u64 << 10) - 1), 0);
    }

    // ── rat ───────────────────────────────────────────────────────────────────

    #[test]
    fn rat_recovers_simple_rationals() {
        assert_eq!(rat_f64(0.1), (1, 10));
        assert_eq!(rat_f64(0.5), (1, 2));
        assert_eq!(rat_f64(3.0), (3, 1));
        let (n, d) = rat_f64(1.0 / 3.0);
        assert_eq!((n, d), (1, 3));
    }

    // ── colon_hp: 0:0.1:1 (Issue #9421) ──────────────────────────────────────

    #[test]
    fn colon_hp_zero_point_one_grid_is_shortest_decimal() {
        let hp = colon_hp(HpElement::F64, 0.0, 0.1, 1.0, 11);
        let expected = [0.0, 0.1, 0.2, 0.3, 0.4, 0.5, 0.6, 0.7, 0.8, 0.9, 1.0];
        for (i, want) in expected.iter().enumerate() {
            assert_eq!(hp.elem(i as i64 + 1), *want, "element {}", i + 1);
        }
        assert_eq!(hp.step_f64(), 0.1);
    }

    #[test]
    fn colon_hp_non_rational_falls_back_to_literal() {
        let step = std::f64::consts::PI;
        let hp = colon_hp(HpElement::F64, 0.0, step, 10.0, 4);
        assert_eq!(hp.elem(1), 0.0);
        assert_eq!(hp.elem(2), step);
        assert_eq!(hp.elem(4), 3.0 * step);
    }

    #[test]
    fn colon_hp_length_float32_uses_float32_rational_len_issue_9510() {
        let start = f64::from(0.1f32);
        let step = f64::from(0.1f32);
        let stop = f64::from(0.5f32);
        assert_eq!(colon_hp_length(HpElement::F32, start, step, stop), Some(5));

        let hp = colon_hp(HpElement::F32, start, step, stop, 5);
        assert_eq!((hp.elem(5) as f32).to_bits(), 0.5f32.to_bits());
    }

    // ── linspace_hp: range(0, 1, length = n) (Issue #9419) ───────────────────

    #[test]
    fn linspace_hp_matches_upstream_values() {
        // julia> collect(range(0, 1, length=3)) == [0.0, 0.5, 1.0]
        let hp = linspace_hp_f64(0.0, 1.0, 3);
        assert_eq!(hp.elem(1), 0.0);
        assert_eq!(hp.elem(2), 0.5);
        assert_eq!(hp.elem(3), 1.0);
        // julia> collect(range(0, 1, length=7))[3] == 0.3333333333333333
        let hp = linspace_hp_f64(0.0, 1.0, 7);
        assert_eq!(hp.elem(3), 0.3333333333333333);
        assert_eq!(hp.elem(7), 1.0);
        assert_eq!(hp.step_f64(), 0.16666666666666666);
    }

    #[test]
    fn linspace_hp_general_endpoints_match_exactly() {
        // Irrational endpoints: first/last still land exactly on the inputs.
        let (a, b) = (0.1, std::f64::consts::PI);
        let hp = linspace_hp_f64(a, b, 5);
        assert_eq!(hp.elem(1), a);
        assert_eq!(hp.elem(5), b);
    }

    // ── complex-scaled range: `im .* range(1.2, -1.2; length=1360)` (#9659) ──

    #[test]
    fn complex_scale_matches_upstream_bits_issue_9659() {
        // Ground truth extracted from julia 1.12.6:
        //   ys = range(1.2, -1.2; length=1360); imys = im .* ys
        let hp = linspace_hp_f64(1.2, -1.2, 1360);
        assert_eq!(hp.ref_.hi.to_bits(), 4561283481140758896);
        assert_eq!(hp.ref_.lo.to_bits(), 4291067503498529136);
        assert_eq!(hp.step.hi.to_bits(), 13789159117622904832);
        assert_eq!(hp.step.lo.to_bits(), 13589330524885273921);
        assert_eq!(hp.offset, 680);

        let scaled = hp.scale_complex(C64::new(0.0, 1.0));
        // imys.ref: unchanged parts rotated into the imaginary component.
        assert_eq!(scaled.ref_.hi.re.to_bits(), 0);
        assert_eq!(scaled.ref_.hi.im.to_bits(), 4561283481140758896);
        assert_eq!(scaled.ref_.lo.re.to_bits(), 0);
        assert_eq!(scaled.ref_.lo.im.to_bits(), 4291067503498529136);
        // imys.step: canonicalize2-renormalized by the TwicePrecision multiply.
        assert_eq!(scaled.step.hi.re.to_bits(), 0);
        assert_eq!(scaled.step.hi.im.to_bits(), 13789159117622905200);
        assert_eq!(scaled.step.lo.re.to_bits(), 0);
        assert_eq!(scaled.step.lo.im.to_bits(), 13518943139980705792);

        // Elements — julia `imys[i]` bit patterns. Note imys[11] differs by
        // 1ulp from the elementwise `im * ys[11]` (that discrepancy is the
        // #9659 checksum bug).
        for (i, re_bits, im_bits) in [
            (1_i64, 0_u64, 4608083138725491507_u64),
            (11, 0, 4608003604957237723),
            (680, 0, 4561283481140758896),
            (681, 0, 13784655517995534704),
            (1360, 0, 13831455175580267315),
        ] {
            let v = scaled.elem(i);
            assert_eq!(
                (v.re.to_bits(), v.im.to_bits()),
                (re_bits, im_bits),
                "element {i}"
            );
        }
    }
}
