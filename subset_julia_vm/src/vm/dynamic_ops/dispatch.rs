use crate::rng::RngLike;

use super::super::value::Value;
use super::super::Vm;

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

        if matches!(a, Value::BigInt(_)) || matches!(b, Value::BigInt(_)) {
            return true;
        }

        if let (Value::Array(arr_a), Value::Array(arr_b)) = (a, b) {
            use super::super::value::ArrayData;
            let a_ref = arr_a.borrow();
            let b_ref = arr_b.borrow();
            let a_is_primitive =
                !matches!(a_ref.data, ArrayData::StructRefs(_) | ArrayData::Any(_));
            let b_is_primitive =
                !matches!(b_ref.data, ArrayData::StructRefs(_) | ArrayData::Any(_));
            if a_is_primitive && b_is_primitive {
                return true;
            }
            return false;
        }

        if let Value::Array(arr) = a {
            use super::super::value::ArrayData;
            let arr_ref = arr.borrow();
            if !matches!(arr_ref.data, ArrayData::StructRefs(_) | ArrayData::Any(_)) {
                return true;
            }
            return false;
        }
        if let Value::Array(arr) = b {
            use super::super::value::ArrayData;
            let arr_ref = arr.borrow();
            if !matches!(arr_ref.data, ArrayData::StructRefs(_) | ArrayData::Any(_)) {
                return true;
            }
            return false;
        }

        if matches!(a, Value::Memory(_)) || matches!(b, Value::Memory(_)) {
            return true;
        }

        false
    }
}
