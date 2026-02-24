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
//! - `ops`: Lattice operations (join, meet, subtype, subtract)
//! - `widening`: Constants controlling type widening behavior

pub mod ops;
pub mod types;
pub mod widening;

pub use types::{ConcreteType, LatticeType};
pub use widening::{MAX_INFERENCE_ITERATIONS, MAX_UNION_COMPLEXITY, MAX_UNION_LENGTH};
