use crate::vm::error::VmError;
use crate::vm::value::{ArrayData, ArrayValue, StructInstance};

use super::complex::Complex64;
use super::helpers::extract_complex_data;

/// Scalar-vector multiplication: scalar * vector or vector * scalar.
pub(crate) fn scalar_vector_mul_complex(
    scalar: Complex64,
    arr: &ArrayValue,
    struct_heap: &[StructInstance],
) -> Result<ArrayValue, VmError> {
    let vec_data = extract_complex_data(arr, struct_heap)?;
    let result_data: Vec<Complex64> = vec_data.iter().map(|&v| scalar.mul(v)).collect();
    let result_shape = arr.shape.clone();
    let interleaved: Vec<f64> = result_data.iter().flat_map(|c| [c.re, c.im]).collect();
    Ok(ArrayValue::complex_f64(interleaved, result_shape))
}

/// Scalar-vector multiplication for real arrays: scalar * vector or vector * scalar.
pub(crate) fn scalar_vector_mul_real(scalar: f64, arr: &ArrayValue) -> Result<ArrayValue, VmError> {
    let result_shape = arr.shape.clone();

    let result_data: Vec<f64> = match &arr.data {
        ArrayData::F64(data) => data.iter().map(|&v| scalar * v).collect(),
        ArrayData::I64(data) => data.iter().map(|&v| scalar * v as f64).collect(),
        ArrayData::F32(data) => data.iter().map(|&v| scalar * v as f64).collect(),
        ArrayData::I32(data) => data.iter().map(|&v| scalar * v as f64).collect(),
        ArrayData::I16(data) => data.iter().map(|&v| scalar * v as f64).collect(),
        ArrayData::I8(data) => data.iter().map(|&v| scalar * v as f64).collect(),
        ArrayData::U64(data) => data.iter().map(|&v| scalar * v as f64).collect(),
        ArrayData::U32(data) => data.iter().map(|&v| scalar * v as f64).collect(),
        ArrayData::U16(data) => data.iter().map(|&v| scalar * v as f64).collect(),
        ArrayData::U8(data) => data.iter().map(|&v| scalar * v as f64).collect(),
        _ => {
            return Err(VmError::TypeError(
                "scalar_vector_mul_real: unsupported array element type".to_string(),
            ));
        }
    };

    Ok(ArrayValue::from_f64(result_data, result_shape))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::vm::value::{ArrayData, ArrayValue};

    #[test]
    fn test_scalar_vector_mul_real_f64() {
        let arr = ArrayValue::from_f64(vec![1.0, 2.0, 4.0], vec![3]);
        let result = scalar_vector_mul_real(2.0, &arr).unwrap();
        if let ArrayData::F64(data) = result.data {
            assert_eq!(data, vec![2.0, 4.0, 8.0]);
        }
    }

    #[test]
    fn test_scalar_vector_mul_real_i64() {
        let arr = ArrayValue {
            data: ArrayData::I64(vec![1, 2, 3]),
            shape: vec![3],
            struct_type_id: None,
            element_type_override: None,
        };
        let result = scalar_vector_mul_real(3.0, &arr).unwrap();
        if let ArrayData::F64(data) = result.data {
            assert_eq!(data, vec![3.0, 6.0, 9.0]);
        }
    }

    #[test]
    fn test_scalar_vector_mul_real_preserves_shape() {
        let arr = ArrayValue::from_f64(vec![1.0, 2.0, 3.0, 4.0], vec![2, 2]);
        let result = scalar_vector_mul_real(1.0, &arr).unwrap();
        assert_eq!(result.shape, vec![2, 2]);
    }

    #[test]
    fn test_scalar_vector_mul_real_invalid_type_returns_err() {
        let arr = ArrayValue {
            data: ArrayData::Bool(vec![true, false]),
            shape: vec![2],
            struct_type_id: None,
            element_type_override: None,
        };
        assert!(matches!(
            scalar_vector_mul_real(2.0, &arr),
            Err(VmError::TypeError(_))
        ));
    }

    #[test]
    fn test_scalar_vector_mul_complex_from_real() {
        let arr = ArrayValue::from_f64(vec![1.0, 2.0], vec![2]);
        let scalar = Complex64::from_real(2.0);
        let heap = vec![];
        let result = scalar_vector_mul_complex(scalar, &arr, &heap).unwrap();
        assert_eq!(result.shape, vec![2]);
    }
}
