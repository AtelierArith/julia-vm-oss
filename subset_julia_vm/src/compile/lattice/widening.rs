//! Type widening constants for lattice operations.
//!
//! These constants control when and how Union types are widened to prevent
//! infinite recursion during type inference.

/// Maximum number of elements in a Union type before widening to a supertype.
/// Mirrors Julia's MAX_TYPEUNION_LENGTH.
/// Increased from 4 to 8 to allow more precise type inference for heterogeneous collections.
pub const MAX_UNION_LENGTH: usize = 8;

/// Maximum nesting depth of Union types before widening.
/// Mirrors Julia's MAX_TYPEUNION_COMPLEXITY.
/// Increased from 3 to 5 to allow deeper nested union types.
pub const MAX_UNION_COMPLEXITY: usize = 5;

/// Maximum iterations for fixed-point computation in abstract interpretation.
pub const MAX_INFERENCE_ITERATIONS: usize = 100;
