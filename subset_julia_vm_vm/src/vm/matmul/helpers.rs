use crate::vm::error::VmError;
use crate::vm::value::{ArrayData, ArrayElementType, ArrayValue, StructInstance, Value};

use super::complex::Complex64;

fn is_real_numeric_array(element_type: &ArrayElementType) -> bool {
    matches!(
        element_type,
        ArrayElementType::F64
            | ArrayElementType::F32
            | ArrayElementType::I64
            | ArrayElementType::I32
            | ArrayElementType::I16
            | ArrayElementType::I8
            | ArrayElementType::U64
            | ArrayElementType::U32
            | ArrayElementType::U16
            | ArrayElementType::U8
    )
}

/// Check if an array contains complex numbers.
pub(crate) fn is_complex_array(arr: &ArrayValue) -> bool {
    if let Some(ref override_type) = arr.element_type_override {
        if override_type.is_complex() {
            return true;
        }
    }

    if matches!(arr.data, ArrayData::StructRefs(_)) {
        return true;
    }

    if let ArrayData::Any(values) = &arr.data {
        if !values.is_empty() {
            if let Value::Struct(s) = &values[0] {
                return s.is_complex();
            }
        }
    }

    false
}

/// Helper to extract array data as f64 Vec (for real arrays).
pub(super) fn as_f64_data(arr: &ArrayValue) -> Result<Vec<f64>, VmError> {
    if is_real_numeric_array(&arr.element_type()) {
        arr.to_logical_f64_vec()
    } else {
        Err(VmError::TypeError(format!(
            "matmul requires numeric arrays, got {}",
            arr.data.type_name()
        )))
    }
}

fn complex_from_value(val: Value, struct_heap: &[StructInstance]) -> Result<Complex64, VmError> {
    match val {
        Value::Struct(s) => {
            if s.is_complex() {
                s.as_complex_parts().map_or_else(
                    || {
                        Err(VmError::TypeError(
                            "matmul: could not extract complex parts from struct".to_string(),
                        ))
                    },
                    |(re, im)| Ok(Complex64::new(re, im)),
                )
            } else {
                Err(VmError::TypeError(format!(
                    "matmul: expected Complex struct, got {}",
                    s.struct_name
                )))
            }
        }
        Value::StructRef(idx) => {
            let s = struct_heap.get(idx).ok_or_else(|| {
                VmError::TypeError("matmul: invalid struct reference".to_string())
            })?;
            if s.is_complex() {
                s.as_complex_parts().map_or_else(
                    || {
                        Err(VmError::TypeError(
                            "matmul: could not extract complex parts from struct".to_string(),
                        ))
                    },
                    |(re, im)| Ok(Complex64::new(re, im)),
                )
            } else {
                Err(VmError::TypeError(format!(
                    "matmul: expected Complex struct, got {}",
                    s.struct_name
                )))
            }
        }
        Value::F64(x) => Ok(Complex64::from_real(x)),
        Value::I64(x) => Ok(Complex64::from_real(x as f64)),
        other => Err(VmError::TypeError(format!(
            "matmul: unsupported element type in array: {:?}",
            other
        ))),
    }
}

/// Extract complex data from an array (either real or complex).
pub(crate) fn extract_complex_data(
    arr: &ArrayValue,
    struct_heap: &[StructInstance],
) -> Result<Vec<Complex64>, VmError> {
    if arr.element_type().is_complex()
        || matches!(arr.data, ArrayData::StructRefs(_) | ArrayData::Any(_))
    {
        let mut result = Vec::with_capacity(arr.element_count());
        for i in 0..arr.element_count() {
            result.push(complex_from_value(arr.get_linear(i)?, struct_heap)?);
        }
        return Ok(result);
    }

    let real_data = as_f64_data(arr)?;
    Ok(real_data.into_iter().map(Complex64::from_real).collect())
}

#[cfg(test)]
#[allow(clippy::unwrap_used, clippy::expect_used)]
mod tests {
    use super::*;
    use crate::vm::value::{new_array_ref, ArrayValue};

    fn f64_array(data: Vec<f64>) -> ArrayValue {
        let len = data.len();
        ArrayValue::from_f64(data, vec![len])
    }

    #[test]
    fn test_is_complex_array_real_returns_false() {
        let arr = f64_array(vec![1.0, 2.0, 3.0]);
        assert!(!is_complex_array(&arr));
    }

    #[test]
    fn test_is_complex_array_empty_returns_false() {
        let arr = f64_array(vec![]);
        assert!(!is_complex_array(&arr));
    }

    #[test]
    fn test_as_f64_data_f64_array() {
        let arr = f64_array(vec![1.0, 2.0, 3.0]);
        let result = as_f64_data(&arr).unwrap();
        assert_eq!(result, vec![1.0, 2.0, 3.0]);
    }

    #[test]
    fn test_as_f64_data_i64_array() {
        let arr = ArrayValue::memory_first_from_i64(vec![10, 20, 30], vec![3]);
        let result = as_f64_data(&arr).unwrap();
        assert_eq!(result, vec![10.0, 20.0, 30.0]);
    }

    #[test]
    fn test_as_f64_data_invalid_type_returns_err() {
        let arr = ArrayValue::memory_first_from_bool(vec![true, false], vec![2]);
        assert!(matches!(as_f64_data(&arr), Err(VmError::TypeError(_))));
    }

    #[test]
    fn test_as_f64_data_reads_reshaped_parent_logically() {
        let source = new_array_ref(ArrayValue::memory_first_from_i64(vec![1, 2, 3, 4], vec![4]));
        let reshaped = ArrayValue::reshaped_from_ref(&source, vec![2, 2]).unwrap();

        let result = as_f64_data(&reshaped).unwrap();

        assert_eq!(result, vec![1.0, 2.0, 3.0, 4.0]);
    }

    #[test]
    fn test_extract_complex_data_from_real_array() {
        let arr = f64_array(vec![1.0, 2.0]);
        let heap = vec![];
        let result = extract_complex_data(&arr, &heap).unwrap();
        assert_eq!(result.len(), 2);
        assert_eq!(result[0].re, 1.0);
        assert_eq!(result[1].re, 2.0);
    }

    #[test]
    fn test_extract_complex_data_reads_reshaped_complex_parent_logically() {
        let source = new_array_ref(ArrayValue::complex_f64(vec![1.0, 2.0, 3.0, 4.0], vec![2]));
        let reshaped = ArrayValue::reshaped_from_ref(&source, vec![2]).unwrap();
        let heap = vec![];

        let result = extract_complex_data(&reshaped, &heap).unwrap();

        assert_eq!(result.len(), 2);
        assert_eq!((result[0].re, result[0].im), (1.0, 2.0));
        assert_eq!((result[1].re, result[1].im), (3.0, 4.0));
    }
}
