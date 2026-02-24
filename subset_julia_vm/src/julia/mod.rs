//! Julia standard library modules.
//!
//! This module contains Julia source code that is bundled with the VM:
//! - `base`: Core Julia functions and types (prelude)
//! - `stdlib`: Standard library packages (Random, Statistics, Test, etc.)
//! - `internal`: SubsetJuliaVM-specific modules (AoT prelude, etc.)

pub mod base;
pub mod internal;
pub mod stdlib;
