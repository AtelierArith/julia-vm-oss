//! SubsetJuliaVM-specific internal modules.
//!
//! This module contains Julia source code that is specific to SubsetJuliaVM
//! and is not part of Julia's standard Base library.
//!
//! - `prelude_aot.jl`: Minimal typed prelude for AoT compilation

/// Minimal typed prelude for AoT compilation.
/// Contains only fully-typed functions without runtime Value type dependency.
/// Used with --minimal-prelude flag for pure Rust code generation.
pub const PRELUDE_AOT_JL: &str = include_str!("prelude_aot.jl");

/// Get the minimal AoT prelude for pure Rust code generation.
/// This prelude contains only fully-typed functions without runtime Value type dependency.
/// It includes essential operations for Int64, Float64, and Bool types.
pub fn get_aot_prelude() -> String {
    PRELUDE_AOT_JL.to_string()
}
