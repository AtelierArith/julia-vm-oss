// Prevent accidental debug output in library code (Issue #2888).
// CLI binaries (bin/) may use eprintln!() for user-facing error messages.
#![deny(clippy::print_stderr)]

//! SubsetJuliaVM AoT Runtime Library
//!
//! This crate provides runtime support for AoT (Ahead-of-Time) compiled
//! Julia code. It includes:
//!
//! - `Value` enum for dynamic typing
//! - `RuntimeError` for error handling
//! - Array operations
//! - Dynamic dispatch support
//! - Intrinsic functions (math, I/O, etc.)
//! - Type conversion utilities

pub mod array;
pub mod convert;
pub mod dispatch;
pub mod error;
pub mod intrinsics;
#[path = "../../subset_julia_vm/src/rng.rs"]
pub mod rng;
pub mod value;

/// ABI contract version consumed by generated AoT Rust.
///
/// The compiler emits a compile-time equality check against this value so a
/// generated file cannot silently link with an incompatible runtime crate.
pub const AOT_RUNTIME_ABI_VERSION: usize = 1;

/// Prelude module for convenient imports
///
/// # Example
/// ```
/// use subset_julia_vm_runtime::prelude::*;
/// ```
pub mod prelude {
    pub use super::array::TypedArray;
    pub use super::dispatch::{dynamic_binop, dynamic_call, BinOp};
    pub use super::error::{RuntimeError, RuntimeResult};
    pub use super::intrinsics::*;
    pub use super::value::Value;
}

pub use prelude::*;
