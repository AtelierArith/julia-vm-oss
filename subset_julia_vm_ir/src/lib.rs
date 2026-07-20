//! `subset_julia_vm_ir` — shared span and error types for SubsetJuliaVM.
//!
//! This crate provides the foundational source-location and error types used
//! across the SubsetJuliaVM pipeline.  It sits below the type-system layer
//! (`subset_julia_vm_types`) and above the parser (`subset_julia_vm_parser`),
//! and carries no dependencies on either.
//!
//! See `docs/vm/CRATE_SPLIT.md` (Issue #8655) for the overall crate layout.

pub mod error;
pub mod interned;
pub mod span;

// Convenience re-exports matching the public surface used by downstream crates.
pub use error::{
    IncludeError, SyntaxError, SyntaxIssue, UnsupportedFeature, UnsupportedFeatureKind,
};
pub use interned::InternedStr;
pub use span::Span;
