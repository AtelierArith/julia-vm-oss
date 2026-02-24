use crate::vm::error::VmError;
use crate::vm::value::{new_array_ref, ArrayValue, DictKey, SetValue, Value};

/// Normalize Memory values to Array for set operations.
pub(super) fn normalize_memory(val: Value) -> Value {
    match val {
        Value::Memory(mem) => Value::Array(crate::vm::util::memory_to_array_ref(&mem)),
        other => other,
    }
}

/// Pop a Set from the stack.
pub(super) fn pop_set(stack: &mut Vec<Value>) -> Result<SetValue, VmError> {
    match stack.pop() {
        Some(Value::Set(s)) => Ok(s),
        Some(other) => Err(VmError::TypeError(format!(
            "expected Set, got {:?}",
            crate::vm::util::value_type_name(&other)
        ))),
        None => Err(VmError::StackUnderflow),
    }
}

/// Result of popping a Set-or-Array from the stack.
pub(super) enum SetOrArray {
    Set(SetValue),
    Array(Vec<f64>),
}

/// Pop a Set or Array from the stack, converting Arrays to a comparable form.
pub(super) fn pop_set_or_array(stack: &mut Vec<Value>) -> Result<SetOrArray, VmError> {
    match stack.pop() {
        Some(Value::Set(s)) => Ok(SetOrArray::Set(s)),
        Some(Value::Array(arr)) => {
            let arr_ref = arr.borrow();
            let data = arr_ref.try_as_f64_vec()?;
            Ok(SetOrArray::Array(data))
        }
        Some(Value::Memory(mem)) => {
            let arr = crate::vm::util::memory_to_array_ref(&mem);
            let arr_ref = arr.borrow();
            let data = arr_ref.try_as_f64_vec()?;
            Ok(SetOrArray::Array(data))
        }
        Some(other) => Err(VmError::TypeError(format!(
            "expected Set or Array, got {:?}",
            crate::vm::util::value_type_name(&other)
        ))),
        None => Err(VmError::StackUnderflow),
    }
}

/// Convert an f64 vec to a SetValue using DictKey::I64 for integer values.
pub(super) fn vec_to_set(data: &[f64]) -> SetValue {
    let mut set = SetValue::new();
    for &v in data {
        let as_i64 = v as i64;
        if v == as_i64 as f64 {
            set.insert(DictKey::I64(as_i64));
        }
    }
    set
}

/// Array-based union preserving first-seen order.
pub(super) fn array_union(a: &[f64], b: &[f64]) -> Vec<f64> {
    let mut seen = std::collections::HashSet::new();
    let mut result = Vec::new();
    for &v in a.iter().chain(b.iter()) {
        let bits = v.to_bits();
        if seen.insert(bits) {
            result.push(v);
        }
    }
    result
}

/// Array-based intersect preserving order from a.
pub(super) fn array_intersect(a: &[f64], b: &[f64]) -> Vec<f64> {
    let b_set: std::collections::HashSet<u64> = b.iter().map(|v| v.to_bits()).collect();
    let mut seen = std::collections::HashSet::new();
    let mut result = Vec::new();
    for &v in a {
        let bits = v.to_bits();
        if b_set.contains(&bits) && seen.insert(bits) {
            result.push(v);
        }
    }
    result
}

/// Array-based setdiff preserving order from a.
pub(super) fn array_setdiff(a: &[f64], b: &[f64]) -> Vec<f64> {
    let b_set: std::collections::HashSet<u64> = b.iter().map(|v| v.to_bits()).collect();
    let mut seen = std::collections::HashSet::new();
    let mut result = Vec::new();
    for &v in a {
        let bits = v.to_bits();
        if !b_set.contains(&bits) && seen.insert(bits) {
            result.push(v);
        }
    }
    result
}

/// Array-based symdiff preserving a-then-b order.
pub(super) fn array_symdiff(a: &[f64], b: &[f64]) -> Vec<f64> {
    let a_set: std::collections::HashSet<u64> = a.iter().map(|v| v.to_bits()).collect();
    let b_set: std::collections::HashSet<u64> = b.iter().map(|v| v.to_bits()).collect();
    let mut seen = std::collections::HashSet::new();
    let mut result = Vec::new();

    for &v in a {
        let bits = v.to_bits();
        if !b_set.contains(&bits) && seen.insert(bits) {
            result.push(v);
        }
    }
    for &v in b {
        let bits = v.to_bits();
        if !a_set.contains(&bits) && seen.insert(bits) {
            result.push(v);
        }
    }
    result
}

/// Convert a Vec<f64> result into Value::Array.
pub(super) fn vec_to_array(data: Vec<f64>) -> Value {
    let len = data.len();
    Value::Array(new_array_ref(ArrayValue::from_f64(data, vec![len])))
}
