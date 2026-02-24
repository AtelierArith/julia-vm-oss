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
pub mod value;

/// Prelude module for convenient imports
///
/// # Example
/// ```
/// use subset_julia_vm_runtime::prelude::*;
/// ```
pub mod prelude {
    pub use super::array::TypedArray;
    pub use super::dispatch::{dynamic_binop, dynamic_call, BinOp};
    pub use super::error::RuntimeError;
    pub use super::intrinsics::*;
    pub use super::value::Value;
}

pub use prelude::*;
