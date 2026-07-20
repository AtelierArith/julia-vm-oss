use crate::rng::RngLike;

use super::super::broadcast::Broadcastable;
use super::super::value::{
    array_wrapper_value_to_array_value, native_array_value_ref, ArrayData, ArrayValue,
    StructInstance, Value,
};
use super::super::Vm;

fn complex_parts_from_value(value: Value, struct_heap: &[StructInstance]) -> Option<(f64, f64)> {
    match value {
        Value::Struct(s) => s.as_complex_parts(),
        Value::StructRef(idx) => struct_heap.get(idx).and_then(|s| s.as_complex_parts()),
        _ => None,
    }
}

fn to_interleaved_complex<R: RngLike>(vm: &Vm<R>, arr: &ArrayValue) -> Option<ArrayValue> {
    match &arr.data {
        ArrayData::StructRefs(refs) => {
            let mut data = Vec::with_capacity(refs.len() * 2);
            for &idx in refs {
                let s = vm.struct_heap.get(idx)?;
                let (re, im) = s.as_complex_parts()?;
                data.push(re);
                data.push(im);
            }
            Some(ArrayValue::complex_f64(data, arr.shape.clone()))
        }
        ArrayData::Any(_) => {
            let mut data = Vec::with_capacity(arr.element_count() * 2);
            for i in 0..arr.element_count() {
                let (re, im) = complex_parts_from_value(arr.get_linear(i).ok()?, &vm.struct_heap)?;
                data.push(re);
                data.push(im);
            }
            Some(ArrayValue::complex_f64(data, arr.shape.clone()))
        }
        _ => None,
    }
}

pub(super) fn broadcastable_array_like<R: RngLike>(
    vm: &Vm<R>,
    value: &Value,
) -> Option<Broadcastable> {
    if let Some(arr) = native_array_value_ref(value) {
        let arr_ref = arr.borrow();
        let arr_val = to_interleaved_complex(vm, &arr_ref).unwrap_or_else(|| arr_ref.clone());
        return Some(Broadcastable::Array(arr_val));
    }
    if let Value::Memory(mem) = value {
        return Some(Broadcastable::Memory(mem.clone()));
    }
    if let Ok(Some(arr)) = array_wrapper_value_to_array_value(value, &vm.struct_heap) {
        let arr_val = to_interleaved_complex(vm, &arr).unwrap_or(arr);
        return Some(Broadcastable::Array(arr_val));
    }
    None
}

#[cfg(test)]
#[allow(clippy::unwrap_used, clippy::expect_used)]
mod tests {
    use crate::rng::StableRng;
    use crate::vm::value::{ArrayElementType, StructInstance};

    use super::*;

    #[test]
    fn complex_struct_array_bridge_uses_memory_first_complex_array() {
        let mut vm = Vm::new(vec![], StableRng::new(0));
        vm.struct_heap.push(StructInstance::complex(7, 1.0, 2.0));
        vm.struct_heap.push(StructInstance::complex(7, 3.0, 4.0));

        let arr = ArrayValue::new(ArrayData::StructRefs(vec![0, 1]), vec![2]);
        let converted = to_interleaved_complex(&vm, &arr).unwrap();

        assert_eq!(converted.shape, vec![2]);
        assert_eq!(converted.element_type(), ArrayElementType::ComplexF64);
        assert_eq!(
            converted.element_type_override,
            Some(ArrayElementType::ComplexF64)
        );
        assert!(converted.shared_parent.is_none());
        assert!(converted.struct_type_id.is_none());
        // Issue #9198 S5: Complex{Float64} arrays back their interleaved buffer
        // with the general contiguous-isbits `StructF64` variant (was `F64`).
        match converted.data {
            ArrayData::StructF64(data) => assert_eq!(data, vec![1.0, 2.0, 3.0, 4.0]),
            other => panic!("expected interleaved StructF64 storage, got {:?}", other),
        }
    }
}
