//! Lattice-based type system for abstract interpretation.
//!
//! This module provides the foundation for type inference in SubsetJuliaVM
//! using abstract interpretation. The type lattice enables precise type
//! tracking through control flow and supports advanced features like:
//!
//! - Union types with automatic widening
//! - Conditional types for type narrowing
//! - Control-flow sensitive type inference
//!
//! # Module structure
//!
//! - `types`: Core lattice type definitions (`LatticeType`, `ConcreteType`)
//! - `abstract_lattice`: The `AbstractLattice` trait — the unified home for
//!   the meet/join/widen algebra (Issue #6605), mirroring upstream Julia's
//!   `AbstractLattice` abstraction
//! - `ops`: Lattice operation bodies (join, meet, subtype, subtract, widen)
//! - `widening`: Constants controlling type widening behavior

#![deny(clippy::unwrap_used)]
#![deny(clippy::expect_used)]

pub mod abstract_lattice;
pub mod ops;
pub mod types;
pub mod widening;

pub use abstract_lattice::AbstractLattice;
pub use types::{ConcreteType, LatticeType};
pub use widening::{
    limit_type_size, MAX_INFERENCE_ITERATIONS, MAX_UNION_COMPLEXITY, MAX_UNION_LENGTH,
};
