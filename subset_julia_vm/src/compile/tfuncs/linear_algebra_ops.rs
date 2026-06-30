//! Transfer functions for `LinearAlgebra` module calls.
//!
//! Result-shape rules for `LinearAlgebra.f(...)` module-qualified calls
//! (Issue #5922). These rules replace the nested ad-hoc `match` that lived in
//! `compile/expr/infer/mod.rs`'s `Expr::ModuleCall` arm.
//!
//! They are registered under `LinearAlgebra.`-qualified keys (see
//! `register_linear_algebra` in the parent module) so they apply **only** to
//! module-qualified call sites. Bare names (`det`, `transpose`, `inv`, ...)
//! keep their existing builtin-op / method-dispatch routing — user methods may
//! shadow them (fixture `linalg/det_lu_module_dispatch_first_4020.jl` pins the
//! module-qualified result shape even when a user `det(::Array)` overload
//! exists).
//!
//! The legacy gate ignored argument types entirely, so every tfunc here is a
//! constant result-shape rule.

use crate::compile::lattice::types::{ConcreteType, LatticeType};
use crate::inference_core::{CorePrimitive, CoreType};

/// `LinearAlgebra.det` / `LinearAlgebra.cond`: scalar `Float64` result.
pub fn tfunc_la_float64(_args: &[LatticeType]) -> LatticeType {
    LatticeType::Concrete(ConcreteType::Core(CoreType::Primitive(
        CorePrimitive::Float64,
    )))
}

/// `LinearAlgebra.rank`: `Int64` result.
pub fn tfunc_la_int64(_args: &[LatticeType]) -> LatticeType {
    LatticeType::Concrete(ConcreteType::Core(CoreType::Primitive(
        CorePrimitive::Int64,
    )))
}

/// `LinearAlgebra.svd` / `qr` / `eigen` / `cholesky`: factorization results
/// surface as named tuples (field names are not statically tracked here).
pub fn tfunc_la_named_tuple(_args: &[LatticeType]) -> LatticeType {
    LatticeType::Concrete(ConcreteType::NamedTuple { fields: Vec::new() })
}

/// `LinearAlgebra.lu`: `(L, U, p)` tuple result.
pub fn tfunc_la_tuple(_args: &[LatticeType]) -> LatticeType {
    LatticeType::Concrete(ConcreteType::Tuple {
        elements: Vec::new(),
    })
}

/// `LinearAlgebra.inv` / `eigvals` / `transpose`: array result with a
/// statically unknown element type.
pub fn tfunc_la_array(_args: &[LatticeType]) -> LatticeType {
    LatticeType::Concrete(ConcreteType::Array {
        element: Box::new(ConcreteType::Core(CoreType::Any)),
        ndims: None,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn linear_algebra_result_shapes_are_constant() {
        assert_eq!(
            tfunc_la_float64(&[]),
            LatticeType::Concrete(ConcreteType::Core(CoreType::Primitive(
                CorePrimitive::Float64
            )))
        );
        assert_eq!(
            tfunc_la_int64(&[LatticeType::Top]),
            LatticeType::Concrete(ConcreteType::Core(CoreType::Primitive(
                CorePrimitive::Int64
            )))
        );
        assert_eq!(
            tfunc_la_named_tuple(&[]),
            LatticeType::Concrete(ConcreteType::NamedTuple { fields: Vec::new() })
        );
        assert_eq!(
            tfunc_la_tuple(&[]),
            LatticeType::Concrete(ConcreteType::Tuple {
                elements: Vec::new()
            })
        );
        assert_eq!(
            tfunc_la_array(&[]),
            LatticeType::Concrete(ConcreteType::Array {
                element: Box::new(ConcreteType::Core(CoreType::Any)),
                ndims: None
            })
        );
    }
}
