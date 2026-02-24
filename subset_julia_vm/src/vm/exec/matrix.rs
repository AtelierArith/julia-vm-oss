//! Matrix operation instructions.
//!
//! Handles: MatMul
//! Note: Adjoint and Transpose have been migrated to Pure Julia

#![deny(clippy::unwrap_used)]
#![deny(clippy::expect_used)]

use super::super::matmul::{is_complex_array, matmul, matmul_complex};
use super::super::*;
use super::stack_ops::StackOps;
use crate::rng::RngLike;

/// Result type for matrix operations
pub(super) enum MatrixResult {
    /// Instruction was not handled by this module
    NotHandled,
    /// Instruction was handled successfully
    Handled,
    /// Instruction handled, but need to continue (e.g., after raise)
    Continue,
}

impl<R: RngLike> Vm<R> {
    /// Execute matrix operation instructions.
    ///
    /// Returns `MatrixResult::NotHandled` if the instruction is not a matrix operation.
    #[inline]
    pub(super) fn execute_matrix(&mut self, instr: &Instr) -> Result<MatrixResult, VmError> {
        match instr {
            Instr::MatMul => {
                let b_result = self.stack.pop_array();
                let b = match self.try_or_handle(b_result)? {
                    Some(arr) => arr,
                    None => return Ok(MatrixResult::Continue),
                };
                let a_result = self.stack.pop_array();
                let a = match self.try_or_handle(a_result)? {
                    Some(arr) => arr,
                    None => return Ok(MatrixResult::Continue),
                };

                // Check if either array contains complex numbers
                let a_borrowed = a.borrow();
                let b_borrowed = b.borrow();
                let a_is_complex = is_complex_array(&a_borrowed);
                let b_is_complex = is_complex_array(&b_borrowed);

                let mul_result = if a_is_complex || b_is_complex {
                    // Use complex-aware matmul with access to struct_heap
                    matmul_complex(&a_borrowed, &b_borrowed, &self.struct_heap)
                } else {
                    // Use standard real matmul
                    matmul(&a_borrowed, &b_borrowed)
                };
                drop(a_borrowed);
                drop(b_borrowed);

                let mut result = match self.try_or_handle(mul_result)? {
                    Some(result) => result,
                    None => return Ok(MatrixResult::Continue),
                };
                // Store correct Complex type_id for complex array results (Issue #1804)
                if result
                    .element_type_override
                    .as_ref()
                    .is_some_and(|e| e.is_complex())
                {
                    result.struct_type_id = Some(self.get_complex_type_id());
                }
                self.stack.push(Value::Array(new_array_ref(result)));
                Ok(MatrixResult::Handled)
            }

            // Note: Instr::Adjoint and Instr::Transpose have been removed
            // They are now implemented in Pure Julia:
            // - subset_julia_vm/src/julia/base/array.jl (for arrays)
            // - subset_julia_vm/src/julia/base/number.jl (for scalars)
            // - subset_julia_vm/src/julia/base/complex.jl (for Complex numbers)
            _ => Ok(MatrixResult::NotHandled),
        }
    }
}
