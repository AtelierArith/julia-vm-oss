//! Type inference for AoT compilation.
//!
//! # Module Organization
//!
//! - `types.rs`: Type definitions (FunctionSignature, TypedProgram, InferenceResult, etc.)
//! - `engine.rs`: TypeInferenceEngine struct and all inference methods
//! - `tests.rs`: Comprehensive test suite

mod engine;
#[cfg(test)]
mod tests;
pub mod types;

// Re-export all public types
pub use engine::TypeInferenceEngine;
pub use types::{
    CallSite, FunctionSignature, InferenceResult, StructTypeInfo, TypeEnv, TypedFunction,
    TypedProgram,
};
