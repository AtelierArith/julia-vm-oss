//! Core IR loading and AoT IR conversion entry points.
//!
//! Provides convenience functions for loading persisted Core IR files and
//! converting programs to AoT IR.

use super::super::inference::{TypeInferenceEngine, TypedProgram};
use super::super::ir::AotProgram;
use super::super::{AotError, AotResult};
use super::ir_converter::IrConverter;
use crate::aot::call_graph::CallGraph;
use crate::ir::core::Program;
use std::path::Path;

// ============================================================================
// Core IR File Loading
// ============================================================================

/// Load a Core IR file (.sjir) and return the Core IR Program.
///
/// This function loads a Core IR file that was created by `sjulia --compile`
/// and returns the Core IR representation suitable for AoT compilation.
///
/// # Arguments
///
/// * `path` - Path to the Core IR file
///
/// # Returns
///
/// Returns the loaded Program on success, or an AotError on failure.
///
/// # Example
///
/// ```ignore
/// let program = load_ir_file("program.sjir")?;
/// let typed = engine.analyze_program(&program)?;
/// let aot_ir = program_to_aot_ir(&program, &typed)?;
/// ```
pub fn load_ir_file<P: AsRef<Path>>(path: P) -> AotResult<Program> {
    crate::core_ir_file::load(path)
        .map_err(|e| AotError::InternalError(format!("Failed to load Core IR: {}", e)))
}

/// Load Core IR from raw persisted bytes and return the Core IR Program.
///
/// This function loads Core IR from an in-memory buffer and returns
/// the Core IR representation suitable for AoT compilation.
///
/// # Arguments
///
/// * `data` - Raw Core IR bytes
///
/// # Returns
///
/// Returns the loaded Program on success, or an AotError on failure.
pub fn load_ir_bytes(data: &[u8]) -> AotResult<Program> {
    crate::core_ir_file::load_from_bytes(data)
        .map_err(|e| AotError::InternalError(format!("Failed to load Core IR: {}", e)))
}

/// Convert a Core IR file directly to AoT IR Program.
///
/// This is a convenience function that combines loading and conversion steps.
///
/// # Arguments
///
/// * `path` - Path to the Core IR file
///
/// # Returns
///
/// Returns the AoT IR Program on success, or an AotError on failure.
pub fn ir_file_to_aot_ir<P: AsRef<Path>>(path: P) -> AotResult<AotProgram> {
    // Load the Core IR file
    let program = load_ir_file(path)?;

    // Match the AoT CLI path: persisted Core IR commonly includes prelude
    // functions whose bodies contain internal sentinels such as required-keyword
    // `Undef` markers. Convert only the reachable program surface instead of
    // treating those sentinel-only implementation details as executable values.
    let call_graph = CallGraph::from_program(&program);
    let program = call_graph.filter_program(&program);

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
