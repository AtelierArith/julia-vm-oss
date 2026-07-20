//! Type operations for the VM.
//!
//! This module contains methods for type checking, type conversion,
//! iteration, and deep copying of values.

#![deny(clippy::unwrap_used)]
#![deny(clippy::expect_used)]

mod comparison;
mod conversion;
mod deep_copy;
mod introspection;
mod iteration;
