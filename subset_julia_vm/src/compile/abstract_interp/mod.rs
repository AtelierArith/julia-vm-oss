//! Abstract interpretation for type inference.
//!
//! This module provides the infrastructure for abstract interpretation-based
//! type inference in SubsetJuliaVM. Abstract interpretation tracks types through
//! control flow, supporting advanced features like:
//!
//! - Type narrowing in conditionals (isa, === nothing)
//! - Loop variable type inference
//! - Function return type inference
//! - Union type widening
//!
//! # Module structure
//!
//! - `env`: Type environment for tracking variable types
//! - `engine`: Abstract interpretation engine with fixpoint iteration
//! - `conditional`: Conditional type narrowing for isa and nothing checks
//! - `loop_analysis`: Loop variable type inference for ForEach
//! - `struct_info`: Struct type information with LatticeType fields
//!
//! # Usage
//!
//! ```
//! use subset_julia_vm::compile::abstract_interp::{InferenceEngine, TypeEnv};
//! use subset_julia_vm::compile::lattice::types::{ConcreteType, LatticeType};
//!
//! let mut engine = InferenceEngine::new();
//!
//! // Infer function return type
//! // let return_type = engine.infer_function(&function);
//! ```

pub mod conditional;
pub mod engine;
pub mod env;
pub mod loop_analysis;
pub mod struct_info;
pub mod usage_analysis;

pub use conditional::split_env_by_condition;
pub use engine::InferenceEngine;
pub use env::TypeEnv;
pub use loop_analysis::element_type;
pub use struct_info::{value_type_to_lattice_with_table, StructTypeInfo};
pub use usage_analysis::infer_parameter_constraints;
