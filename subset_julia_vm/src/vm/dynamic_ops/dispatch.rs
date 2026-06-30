use crate::rng::RngLike;

use super::super::value::{native_array_value_ref, Value};
use super::super::Vm;
use super::{is_bigfloat_pow, is_integer_pow_operand};

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
    pub(crate) fn should_use_inline_dynamic_pow(&self, a: &Value, b: &Value) -> bool {
        self.should_use_inline_dynamic_op(a, b)
            || (is_integer_pow_operand(a) && is_integer_pow_operand(b))
            || is_bigfloat_pow(a, b)
    }
}
