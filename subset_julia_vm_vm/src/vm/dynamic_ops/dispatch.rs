use crate::rng::RngLike;

use super::super::value::{native_array_value_ref, Value};
use super::super::Vm;
use super::{is_bigfloat_pow, is_integer_pow_operand};

/// Returns true if `value` is a primitive float (F16, F32, or F64).
///
/// Used to route `Float^Integer` to the inline `dynamic_pow` path (Issue #9155).
fn is_float_value(value: &Value) -> bool {
    matches!(value, Value::F16(_) | Value::F32(_) | Value::F64(_))
}

/// Returns true only if `value` is a genuine `Complex{Float64}` (both fields
/// `F64`).
///
/// Used by [`Vm::should_use_inline_dynamic_pow`] to route `Complex{Float64}^Integer`
/// to the Rust binary-exponentiation fast path (Issue #9155). It must be strict:
/// the inline `dynamic_pow` fast path (`try_complex_f64_int_pow`) only handles
/// `Complex{Float64}` and computes in `f64`. Routing a `Complex{Int}`/`{Bool}`/
/// `{Float32}` base here would send it to `dynamic_pow`, which has no method for
/// those and errors ("Cannot compute power of Complex{Int64} and Int64").
/// Non-`Float64` complex bases must instead reach Julia dispatch, whose
/// `^(z::Complex{<:Integer}, n::Integer)` etc. preserve the component type
/// (Issue #9167).
fn is_complex_f64_value<R: RngLike>(vm: &Vm<R>, value: &Value) -> bool {
    match value {
        Value::Struct(s) => s.complex_f64_parts().is_some(),
        Value::StructRef(idx) => vm
            .struct_heap
            .get(*idx)
            .is_some_and(|s| s.complex_f64_parts().is_some()),
        _ => false,
    }
}

fn is_irrational_value<R: RngLike>(vm: &Vm<R>, value: &Value) -> bool {
    match value {
        Value::Struct(s) => s.as_irrational_f64().is_some(),
        Value::StructRef(idx) => vm
            .struct_heap
            .get(*idx)
            .is_some_and(|s| s.as_irrational_f64().is_some()),
        _ => false,
    }
}

impl<R: RngLike> Vm<R> {
    /// Check if both values should use inline dynamic arithmetic (fast path).
    /// Returns true for same-type primitives, Array, and BigInt operations.
    /// Returns false for mixed-type primitives, Complex, and Rational that should go through
    /// Julia dispatch.
    pub(crate) fn should_use_inline_dynamic_op(&self, a: &Value, b: &Value) -> bool {
        if matches!(
            (a, b),
            (Value::I64(_), Value::I64(_))
                | (Value::F64(_), Value::F64(_))
                | (Value::F32(_), Value::F32(_))
                | (Value::F16(_), Value::F16(_))
                | (Value::Bool(_), Value::Bool(_))
        ) {
            return true;
        }

        if is_irrational_value(self, a) || is_irrational_value(self, b) {
            return true;
        }

        if matches!(a, Value::BigInt(_)) || matches!(b, Value::BigInt(_)) {
            return true;
        }

        if let (Some(arr_a), Some(arr_b)) = (native_array_value_ref(a), native_array_value_ref(b)) {
            let a_ref = arr_a.borrow();
            let b_ref = arr_b.borrow();
            if a_ref.supports_inline_dynamic_storage() && b_ref.supports_inline_dynamic_storage() {
                return true;
            }
            return false;
        }

        if let Some(arr) = native_array_value_ref(a) {
            let arr_ref = arr.borrow();
            if arr_ref.supports_inline_dynamic_storage() {
                return true;
            }
            return false;
        }
        if let Some(arr) = native_array_value_ref(b) {
            let arr_ref = arr.borrow();
            if arr_ref.supports_inline_dynamic_storage() {
                return true;
            }
            return false;
        }

        if matches!(a, Value::Memory(_)) || matches!(b, Value::Memory(_)) {
            return true;
        }

        false
    }

    /// Check if `DynamicPow` should stay on the VM inline path.
    ///
    /// Mixed-width primitive integer powers need this pow-specific route because
    /// generic `^(::Number, ::Integer)` dispatch recursively re-enters `^`.
    /// BigFloat powers (against any real numeric) need it for the same reason:
    /// there is no terminating `^(::BigFloat, …)` method, so dispatch
    /// infinite-recurses → stack overflow (Issue #6790).
    ///
    /// Additionally handled inline (Issue #9155):
    /// - `Float{64,32,16}^Integer` — `pow_f64` is already in the `dynamic_pow`
    ///   match and is faster than the Julia dispatch overhead.
    /// - `Complex{Float64}^Integer` — Rust binary-exponentiation fast path.
    pub(crate) fn should_use_inline_dynamic_pow(&self, a: &Value, b: &Value) -> bool {
        self.should_use_inline_dynamic_op(a, b)
            || (is_integer_pow_operand(a) && is_integer_pow_operand(b))
            || is_bigfloat_pow(a, b)
            // Float^Integer: dynamic_pow already has inline arms for all
            // F64/F32/F16 base × integer exponent combinations (Issue #9155).
            || (is_float_value(a) && is_integer_pow_operand(b))
            // Complex{Float64}^Integer: Rust binary-exponentiation (Issue #9155).
            // Strict Complex{Float64} only — other component types go to Julia
            // dispatch, which preserves the component type (Issue #9167).
            // Issue #9198 S6: bypassable for the retirement A/B measurement — when
            // disabled the clause is false, so Complex{Float64}^Integer routes to
            // Julia dispatch (identical to the eventual deletion of this clause).
            || (!crate::vm::complex_fastpath_gate::complex_fastpath_disabled()
                && is_complex_f64_value(self, a)
                && is_integer_pow_operand(b))
    }
}
