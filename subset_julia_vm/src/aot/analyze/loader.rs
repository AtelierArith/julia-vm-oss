//! Bytecode loading and AoT IR conversion entry points.
//!
//! Provides convenience functions for loading bytecode files and
//! converting programs to AoT IR.

use super::super::inference::{TypeInferenceEngine, TypedProgram};
use super::super::ir::AotProgram;
use super::super::{AotError, AotResult};
use super::ir_converter::IrConverter;
use crate::ir::core::Program;
use std::path::Path;

// ============================================================================
// Bytecode File Loading
// ============================================================================

/// Load a bytecode file (.sjbc) and return the Core IR Program
///
/// This function loads a bytecode file that was created by `sjulia --compile`
/// and returns the Core IR representation suitable for AoT compilation.
///
/// # Arguments
///
/// * `path` - Path to the bytecode file
///
/// # Returns
///
/// Returns the loaded Program on success, or an AotError on failure.
///
/// # Example
///
/// ```ignore
/// let program = load_bytecode_file("program.sjbc")?;
/// let typed = engine.analyze_program(&program)?;
/// let aot_ir = program_to_aot_ir(&program, &typed)?;
/// ```
pub fn load_bytecode_file<P: AsRef<Path>>(path: P) -> AotResult<Program> {
    crate::bytecode::load(path)
        .map_err(|e| AotError::InternalError(format!("Failed to load bytecode: {}", e)))
}

/// Load bytecode from raw bytes and return the Core IR Program
///
/// This function loads bytecode from an in-memory buffer and returns
/// the Core IR representation suitable for AoT compilation.
///
/// # Arguments
///
/// * `data` - Raw bytecode bytes
///
/// # Returns
///
/// Returns the loaded Program on success, or an AotError on failure.
pub fn load_bytecode_bytes(data: &[u8]) -> AotResult<Program> {
    crate::bytecode::load_from_bytes(data)
        .map_err(|e| AotError::InternalError(format!("Failed to load bytecode: {}", e)))
}

/// Convert a bytecode file directly to AoT IR Program
///
/// This is a convenience function that combines loading and conversion steps.
///
/// # Arguments
///
/// * `path` - Path to the bytecode file
///
/// # Returns
///
/// Returns the AoT IR Program on success, or an AotError on failure.
pub fn bytecode_file_to_aot_ir<P: AsRef<Path>>(path: P) -> AotResult<AotProgram> {
    // Load the bytecode file
    let program = load_bytecode_file(path)?;

    // Run type inference
    let mut engine = TypeInferenceEngine::new();
    let typed = engine.analyze_program(&program)?;

    // Convert to AoT IR
    program_to_aot_ir(&program, &typed)
}

// ============================================================================
// Core IR to AoT IR Conversion
// ============================================================================

/// Convert Core IR Program to AoT IR Program
///
/// This is the main entry point for converting a Julia Core IR program
/// to the AoT IR representation suitable for Rust code generation.
pub fn program_to_aot_ir(program: &Program, typed: &TypedProgram) -> AotResult<AotProgram> {
    let mut converter = IrConverter::new(typed, program);
    converter.convert_program(program)
}
