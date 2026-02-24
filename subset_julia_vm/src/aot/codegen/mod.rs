//! Code generation for AoT compilation
//!
//! This module provides the code generation infrastructure
//! for transforming IR into executable code.
//!
//! # Backends
//!
//! - **Rust**: Generates Rust source code that can be compiled with `rustc`
//! - **Cranelift** (optional): Generates native code directly using Cranelift JIT

pub mod aot_codegen;
pub mod ir_codegen;

#[cfg(feature = "cranelift")]
pub mod cranelift;

use super::ir::{IrFunction, IrModule};
use super::AotResult;

/// Trait for code generators
pub trait CodeGenerator {
    /// Target language name
    fn target_name(&self) -> &str;

    /// Generate code for a function
    fn generate_function(&mut self, func: &IrFunction) -> AotResult<String>;

    /// Generate code for a module
    fn generate_module(&mut self, module: &IrModule) -> AotResult<String>;
}

/// Configuration for code generation
#[derive(Debug, Clone)]
pub struct CodegenConfig {
    /// Whether to generate debug assertions
    pub debug_assertions: bool,
    /// Whether to generate inline runtime checks
    pub runtime_checks: bool,
    /// Whether to generate comments
    pub emit_comments: bool,
    /// Indentation string
    pub indent: String,
    /// Whether to require fully static types (no Value type dependency)
    /// When true, code generation will fail if any dynamic dispatch is needed
    pub pure_rust: bool,
}

impl Default for CodegenConfig {
    fn default() -> Self {
        Self {
            debug_assertions: cfg!(debug_assertions),
            runtime_checks: true,
            emit_comments: true,
            indent: "    ".to_string(),
            pure_rust: false,
        }
    }
}

impl CodegenConfig {
    /// Create a new configuration
    pub fn new() -> Self {
        Self::default()
    }

    /// Create a release configuration (no debug, no comments)
    pub fn release() -> Self {
        Self {
            debug_assertions: false,
            runtime_checks: false,
            emit_comments: false,
            indent: "    ".to_string(),
            pure_rust: false,
        }
    }

    /// Create a pure Rust configuration (no Value type dependency)
    /// This mode requires all types to be statically known and will fail
    /// if any dynamic dispatch is needed.
    pub fn pure_rust() -> Self {
        Self {
            debug_assertions: false,
            runtime_checks: false,
            emit_comments: false,
            indent: "    ".to_string(),
            pure_rust: true,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_codegen_config_default() {
        let config = CodegenConfig::default();
        assert!(config.runtime_checks);
        assert!(config.emit_comments);
    }

    #[test]
    fn test_codegen_config_release() {
        let config = CodegenConfig::release();
        assert!(!config.debug_assertions);
        assert!(!config.runtime_checks);
        assert!(!config.emit_comments);
        assert!(!config.pure_rust);
    }

    #[test]
    fn test_codegen_config_pure_rust() {
        let config = CodegenConfig::pure_rust();
        assert!(!config.debug_assertions);
        assert!(!config.runtime_checks);
        assert!(!config.emit_comments);
        assert!(config.pure_rust);
    }
}
