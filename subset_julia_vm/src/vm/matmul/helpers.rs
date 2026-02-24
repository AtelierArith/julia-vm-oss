use crate::vm::error::VmError;
use crate::vm::value::{ArrayData, ArrayElementType, ArrayValue, StructInstance, Value};

use super::complex::Complex64;

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
    match &arr.data {
        ArrayData::F64(v) => Ok(v.clone()),
        ArrayData::I64(v) => Ok(v.iter().map(|&x| x as f64).collect()),
        ArrayData::F32(v) => Ok(v.iter().map(|&x| x as f64).collect()),
        ArrayData::I32(v) => Ok(v.iter().map(|&x| x as f64).collect()),
        ArrayData::I16(v) => Ok(v.iter().map(|&x| x as f64).collect()),
        ArrayData::I8(v) => Ok(v.iter().map(|&x| x as f64).collect()),
        ArrayData::U64(v) => Ok(v.iter().map(|&x| x as f64).collect()),
        ArrayData::U32(v) => Ok(v.iter().map(|&x| x as f64).collect()),
        ArrayData::U16(v) => Ok(v.iter().map(|&x| x as f64).collect()),
        ArrayData::U8(v) => Ok(v.iter().map(|&x| x as f64).collect()),
        _ => Err(VmError::TypeError(format!(
            "matmul requires numeric arrays, got {}",
            arr.data.type_name()
        ))),
    }
}

/// Extract complex data from an array (either real or complex).
pub(crate) fn extract_complex_data(
    arr: &ArrayValue,
    struct_heap: &[StructInstance],
) -> Result<Vec<Complex64>, VmError> {
    if let Some(ref override_type) = arr.element_type_override {
        if *override_type == ArrayElementType::ComplexF64 {
            if let ArrayData::F64(v) = &arr.data {
                let mut result = Vec::with_capacity(v.len() / 2);
                for chunk in v.chunks(2) {
                    if chunk.len() == 2 {
                        result.push(Complex64::new(chunk[0], chunk[1]));
                    }
                }
                return Ok(result);
            }
        } else if *override_type == ArrayElementType::ComplexF32 {
            if let ArrayData::F32(v) = &arr.data {
                let mut result = Vec::with_capacity(v.len() / 2);
                for chunk in v.chunks(2) {
                    if chunk.len() == 2 {
                        result.push(Complex64::new(chunk[0] as f64, chunk[1] as f64));
                    }
                }
                return Ok(result);
            }
        }
    }

    if let ArrayData::StructRefs(indices) = &arr.data {
        let mut result = Vec::with_capacity(indices.len());
        for &idx in indices {
            if let Some(s) = struct_heap.get(idx) {
                if s.is_complex() {
                    if let Some((re, im)) = s.as_complex_parts() {
                        result.push(Complex64::new(re, im));
                    } else {
                        return Err(VmError::TypeError(
                            "matmul: could not extract complex parts from struct".to_string(),
                        ));
                    }
                } else {
                    return Err(VmError::TypeError(format!(
                        "matmul: expected Complex struct, got {}",
                        s.struct_name
                    )));
                }
            } else {
                return Err(VmError::TypeError(
                    "matmul: invalid struct reference".to_string(),
                ));
            }
        }
        return Ok(result);
    }

    if let ArrayData::Any(values) = &arr.data {
        let mut result = Vec::with_capacity(values.len());
        for val in values {
            match val {
                Value::Struct(s) => {
                    if s.is_complex() {
                        if let Some((re, im)) = s.as_complex_parts() {
                            result.push(Complex64::new(re, im));
                        } else {
                            return Err(VmError::TypeError(
                                "matmul: could not extract complex parts from struct".to_string(),
                            ));
                        }
                    } else {
                        return Err(VmError::TypeError(format!(
                            "matmul: expected Complex struct, got {}",
                            s.struct_name
                        )));
                    }
                }
                Value::F64(x) => result.push(Complex64::from_real(*x)),
                Value::I64(x) => result.push(Complex64::from_real(*x as f64)),
                _ => {
                    return Err(VmError::TypeError(format!(
                        "matmul: unsupported element type in Any array: {:?}",
                        val
                    )))
                }
            }
        }
        return Ok(result);
    }

    let real_data = as_f64_data(arr)?;
    Ok(real_data.into_iter().map(Complex64::from_real).collect())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::vm::value::{ArrayData, ArrayValue};

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
        let data = vec![10i64, 20, 30];
        let arr = ArrayValue {
            data: ArrayData::I64(data),
            shape: vec![3],
            struct_type_id: None,
            element_type_override: None,
        };
        let result = as_f64_data(&arr).unwrap();
        assert_eq!(result, vec![10.0, 20.0, 30.0]);
    }

    #[test]
    fn test_as_f64_data_invalid_type_returns_err() {
        let arr = ArrayValue {
            data: ArrayData::Bool(vec![true, false]),
            shape: vec![2],
            struct_type_id: None,
            element_type_override: None,
        };
        assert!(matches!(as_f64_data(&arr), Err(VmError::TypeError(_))));
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
}
