//! Code generation for AoT compilation
//!
//! This module provides the code generation infrastructure
//! for transforming IR into executable code.
//!
//! # Backends
//!
//! - **Rust**: Generates Rust source code that can be compiled with `rustc`
//! - **Cranelift** (optional): Generates native code directly using Cranelift JIT
//! - **Wasm** (optional): Generates standalone core WebAssembly modules

pub mod aot_codegen;
pub mod ir_codegen;

#[cfg(feature = "cranelift")]
pub mod cranelift;

#[cfg(feature = "aot-wasm")]
pub mod wasm;

use super::ir::{IrFunction, IrModule};
use super::optimizer::OptLevel;
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
    /// Emit native debug information for backends that support it.
    pub debug_info: bool,
    /// Source file name used in debug information.
    pub source_name: String,
    /// Indentation string
    pub indent: String,
    /// Whether to require fully static types (no Value type dependency)
    /// When true, code generation will fail if any dynamic dispatch is needed
    pub pure_rust: bool,
    /// User-selected AoT optimization level (`-O0` through `-O3`).
    pub opt_level: OptLevel,
    /// Explicit C ABI entry points to export from generated Rust (Issue #6990).
    pub c_abi_exports: Vec<CAbiExport>,
}

/// Request to expose one generated AoT function through a C ABI symbol.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CAbiExport {
    /// Exported symbol name used by `#[no_mangle] extern "C"`.
    pub export_name: String,
    /// Julia/generated function name to expose.
    pub function_name: String,
    /// Optional Julia argument types used to resolve overloaded functions.
    pub arg_types: Option<Vec<super::types::StaticType>>,
}

impl CAbiExport {
    pub fn new(export_name: impl Into<String>, function_name: impl Into<String>) -> Self {
        Self {
            export_name: export_name.into(),
            function_name: function_name.into(),
            arg_types: None,
        }
    }

    pub fn with_arg_types(
        export_name: impl Into<String>,
        function_name: impl Into<String>,
        arg_types: Vec<super::types::StaticType>,
    ) -> Self {
        Self {
            export_name: export_name.into(),
            function_name: function_name.into(),
            arg_types: Some(arg_types),
        }
    }
}

impl Default for CodegenConfig {
    fn default() -> Self {
        Self {
            debug_assertions: cfg!(debug_assertions),
            runtime_checks: true,
            emit_comments: true,
            debug_info: false,
            source_name: "<ir>".to_string(),
            indent: "    ".to_string(),
            pure_rust: false,
            opt_level: OptLevel::default(),
            c_abi_exports: Vec::new(),
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
            debug_info: false,
            source_name: "<ir>".to_string(),
            indent: "    ".to_string(),
            pure_rust: false,
            opt_level: OptLevel::default(),
            c_abi_exports: Vec::new(),
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
            debug_info: false,
            source_name: "<ir>".to_string(),
            indent: "    ".to_string(),
            pure_rust: true,
            opt_level: OptLevel::default(),
            c_abi_exports: Vec::new(),
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
        assert_eq!(config.opt_level, OptLevel::O2);
    }

    #[test]
    fn test_codegen_config_release() {
        let config = CodegenConfig::release();
        assert!(!config.debug_assertions);
        assert!(!config.runtime_checks);
        assert!(!config.emit_comments);
        assert!(!config.pure_rust);
        assert_eq!(config.opt_level, OptLevel::O2);
    }

    #[test]
    fn test_codegen_config_pure_rust() {
        let config = CodegenConfig::pure_rust();
        assert!(!config.debug_assertions);
        assert!(!config.runtime_checks);
        assert!(!config.emit_comments);
        assert!(config.pure_rust);
        assert_eq!(config.opt_level, OptLevel::O2);
    }
}
