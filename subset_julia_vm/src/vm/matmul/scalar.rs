use crate::vm::error::VmError;
use crate::vm::value::{ArrayValue, StructInstance};

use super::complex::Complex64;
use super::helpers::{as_f64_data, extract_complex_data};

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

    let result_data: Vec<f64> = as_f64_data(arr)?.into_iter().map(|v| scalar * v).collect();

    Ok(ArrayValue::memory_first_from_f64(result_data, result_shape))
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
        let arr = ArrayValue::memory_first_from_i64(vec![1, 2, 3], vec![3]);
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
        let arr = ArrayValue::memory_first_from_bool(vec![true, false], vec![2]);
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
