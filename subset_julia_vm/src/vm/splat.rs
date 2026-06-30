//! Shared splat expansion helper for function call handlers.
//!
//! This module centralizes the logic for expanding splatted arguments
//! (`f(args...)`) from Array, Tuple, and Range values into flat argument lists.
//! Used by `call.rs`, `call_dynamic.rs`, and `sync_exec.rs`.

use super::error::VmError;
use super::util::{extract_base_type, is_dict_type_name, strip_module_prefix};
use super::value::{
    array_wrapper_value_to_array_value, native_array_value_ref, StructInstance, Value,
};

const DICT_FILLED_MASK: u8 = 128;

fn is_set_type_name(type_name: &str) -> bool {
    strip_module_prefix(extract_base_type(type_name)) == "Set"
}

fn struct_instance_from_value<'a>(
    value: &'a Value,
    struct_heap: &'a [StructInstance],
) -> Result<Option<&'a StructInstance>, VmError> {
    match value {
        Value::Struct(instance) => Ok(Some(instance)),
        Value::StructRef(idx) => struct_heap
            .get(*idx)
            .map(Some)
            .ok_or_else(|| VmError::TypeError(format!("Invalid StructRef index {}", idx))),
        _ => Ok(None),
    }
}

fn memory_len(value: &Value) -> Option<usize> {
    match value {
        Value::Memory(mem) => Some(mem.borrow().len()),
        Value::MemoryRef(memref) => Some(memref.len()),
        _ => None,
    }
}

fn memory_get(value: &Value, index: usize) -> Result<Option<Value>, VmError> {
    match value {
        Value::Memory(mem) => mem.borrow().get(index).map(Some),
        Value::MemoryRef(memref) => memref.get(index).map(Some),
        _ => Ok(None),
    }
}

fn slot_is_filled(slot: &Value) -> bool {
    match slot {
        Value::U8(v) => (v & DICT_FILLED_MASK) != 0,
        Value::I64(v) => match u8::try_from(*v) {
            Ok(byte) => (byte & DICT_FILLED_MASK) != 0,
            Err(_) => false,
        },
        _ => false,
    }
}

fn set_wrapper_value_to_elements(
    value: &Value,
    struct_heap: &[StructInstance],
) -> Result<Option<Vec<Value>>, VmError> {
    let Some(set_instance) = struct_instance_from_value(value, struct_heap)? else {
        return Ok(None);
    };
    if !is_set_type_name(&set_instance.struct_name) {
        return Ok(None);
    }

    let Some(dict_value) = set_instance.values.first() else {
        return Ok(None);
    };
    let Some(dict_instance) = struct_instance_from_value(dict_value, struct_heap)? else {
        return Ok(None);
    };
    if !is_dict_type_name(&dict_instance.struct_name) {
        return Ok(None);
    }

    let Some(slots) = dict_instance.values.first() else {
        return Ok(None);
    };
    let Some(keys) = dict_instance.values.get(1) else {
        return Ok(None);
    };
    let Some(slot_len) = memory_len(slots) else {
        return Ok(None);
    };

    let mut elements = Vec::new();
    for index in 1..=slot_len {
        let Some(slot) = memory_get(slots, index)? else {
            return Ok(None);
        };
        if slot_is_filled(&slot) {
            let Some(key) = memory_get(keys, index)? else {
                return Ok(None);
            };
            elements.push(key);
        }
    }
    Ok(Some(elements))
}

/// Expand splatted arguments into a flat argument list.
///
/// Given a list of arguments and a splat mask indicating which arguments should
/// be expanded, this function produces a flat `Vec<Value>` with splatted
/// collections (Array, Tuple, Range) inlined.
///
/// # Arguments
/// * `args` - The arguments to process (consumed)
/// * `splat_mask` - Boolean mask where `true` at index `i` means `args[i]` should be splatted
///
/// # Returns
/// A flat `Vec<Value>` with splatted arguments expanded inline.
pub fn expand_splat_arguments(args: Vec<Value>, splat_mask: &[bool]) -> Vec<Value> {
    let mut expanded = Vec::new();
    for (idx, arg) in args.into_iter().enumerate() {
        if !splat_mask.get(idx).copied().unwrap_or(false) {
            expanded.push(arg);
            continue;
        }
        // Expand this argument. The native-array branch routes through the
        // shared `native_array_value_ref` so the file no longer pattern-
        // matches on the legacy native-array variant directly
        // (Issue #3908).
        if let Some(arr) = native_array_value_ref(&arg) {
            let borrowed = arr.borrow();
            for i in 0..borrowed.len() {
                if let Ok(val) = borrowed.get(&[(i + 1) as i64]) {
                    expanded.push(val);
                }
            }
            continue;
        }
        match &arg {
            // Tuple and Core.SimpleVector splat their elements (Issue #4722).
            Value::Tuple(tuple) | Value::SimpleVector(tuple) => {
                for elem in &tuple.elements {
                    expanded.push(elem.clone());
                }
            }
            Value::Range(range) => {
                // Julia ranges are inclusive: 1:3 = [1, 2, 3]
                let mut i = range.start;
                while (range.step > 0.0 && i <= range.stop) || (range.step < 0.0 && i >= range.stop)
                {
                    expanded.push(Value::I64(i as i64));
                    i += range.step;
                }
            }
            _ => expanded.push(arg),
        }
    }
    expanded
}

/// Expand splatted arguments, including Pure Julia `Array{T,N}` wrappers that
/// store elements in `Memory`/`MemoryRef`.
pub fn expand_splat_arguments_with_heap(
    args: Vec<Value>,
    splat_mask: &[bool],
    struct_heap: &[StructInstance],
) -> Result<Vec<Value>, VmError> {
    let mut expanded = Vec::new();
    for (idx, arg) in args.into_iter().enumerate() {
        if !splat_mask.get(idx).copied().unwrap_or(false) {
            expanded.push(arg);
            continue;
        }

        if let Some(arr) = native_array_value_ref(&arg) {
            let borrowed = arr.borrow();
            for i in 0..borrowed.len() {
                expanded.push(borrowed.get(&[(i + 1) as i64])?);
            }
            continue;
        }

        if let Some(arr) = array_wrapper_value_to_array_value(&arg, struct_heap)? {
            for i in 0..arr.element_count() {
                expanded.push(arr.get_linear(i)?);
            }
            continue;
        }

        if let Some(elements) = set_wrapper_value_to_elements(&arg, struct_heap)? {
            expanded.extend(elements);
            continue;
        }

        match &arg {
            // Tuple and Core.SimpleVector splat their elements (Issue #4722).
            Value::Tuple(tuple) | Value::SimpleVector(tuple) => {
                for elem in &tuple.elements {
                    expanded.push(elem.clone());
                }
            }
            Value::Range(range) => {
                let mut i = range.start;
                while (range.step > 0.0 && i <= range.stop) || (range.step < 0.0 && i >= range.stop)
                {
                    expanded.push(Value::I64(i as i64));
                    i += range.step;
                }
            }
            _ => expanded.push(arg),
        }
    }
    Ok(expanded)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::vm::value::{RangeValue, TupleValue};

    fn i64_val(v: i64) -> Value {
        Value::I64(v)
    }

    // ── expand_splat_arguments ────────────────────────────────────────────────

    #[test]
    fn test_no_splat_passes_args_through() {
        // Without any splat, args are returned unchanged
        let args = vec![i64_val(1), i64_val(2)];
        let mask = vec![false, false];
        let result = expand_splat_arguments(args, &mask);
        assert_eq!(result.len(), 2);
        assert!(matches!(result[0], Value::I64(1)));
        assert!(matches!(result[1], Value::I64(2)));
    }

    #[test]
    fn test_tuple_splat_expands_elements() {
        // f((1, 2, 3)...) → f(1, 2, 3)
        let tuple = Value::Tuple(TupleValue::new(vec![i64_val(10), i64_val(20)]));
        let args = vec![tuple];
        let mask = vec![true];
        let result = expand_splat_arguments(args, &mask);
        assert_eq!(result.len(), 2);
        assert!(matches!(result[0], Value::I64(10)));
        assert!(matches!(result[1], Value::I64(20)));
    }

    #[test]
    fn test_range_splat_expands_to_integers() {
        // f((1:3)...) → f(1, 2, 3)
        let range = Value::Range(RangeValue::unit_range(1.0, 3.0));
        let args = vec![range];
        let mask = vec![true];
        let result = expand_splat_arguments(args, &mask);
        assert_eq!(result.len(), 3);
        assert!(matches!(result[0], Value::I64(1)));
        assert!(matches!(result[1], Value::I64(2)));
        assert!(matches!(result[2], Value::I64(3)));
    }

    #[test]
    fn test_non_splatted_arg_before_splatted() {
        // f(0, (1, 2)...) → [0, 1, 2]
        let tuple = Value::Tuple(TupleValue::new(vec![i64_val(1), i64_val(2)]));
        let args = vec![i64_val(0), tuple];
        let mask = vec![false, true];
        let result = expand_splat_arguments(args, &mask);
        assert_eq!(result.len(), 3);
        assert!(matches!(result[0], Value::I64(0)));
        assert!(matches!(result[1], Value::I64(1)));
        assert!(matches!(result[2], Value::I64(2)));
    }

    #[test]
    fn test_non_collection_value_with_splat_passes_through() {
        // Non-collection values (scalars) with splat=true are passed through as-is
        let args = vec![i64_val(42)];
        let mask = vec![true];
        let result = expand_splat_arguments(args, &mask);
        assert_eq!(result.len(), 1);
        assert!(matches!(result[0], Value::I64(42)));
    }

    #[test]
    fn test_empty_tuple_splat_produces_no_args() {
        // f(()...) → f()
        let tuple = Value::Tuple(TupleValue::new(vec![]));
        let args = vec![tuple];
        let mask = vec![true];
        let result = expand_splat_arguments(args, &mask);
        assert_eq!(result.len(), 0);
    }

    #[test]
    fn test_splat_mask_shorter_than_args_treats_extras_as_false() {
        // mask only covers first arg; second arg is NOT splatted
        let args = vec![i64_val(1), i64_val(2)];
        let mask = vec![false]; // shorter than args
        let result = expand_splat_arguments(args, &mask);
        assert_eq!(result.len(), 2);
    }
}
