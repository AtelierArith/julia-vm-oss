//! Matrix and scalar-array operations for the VM.

mod complex;
mod helpers;
mod multiply;
mod scalar;

pub(crate) use complex::Complex64;
pub(crate) use helpers::is_complex_array;
pub(crate) use multiply::{matmul, matmul_complex};
pub(crate) use scalar::{scalar_vector_mul_complex, scalar_vector_mul_real};
