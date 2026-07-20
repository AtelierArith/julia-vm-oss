//! Core IR analysis and IR conversion for AoT compilation.
//!
//! # Module Organization
//!
//! - `core_ir_analyzer.rs`: CoreIrAnalyzer for program analysis
//! - `ir_converter/`: IrConverter split by conversion responsibility (expr/stmt/helpers)
//! - `loader.rs`: Core IR loading and conversion entry points
//! - `tests.rs`: Comprehensive test suite

mod core_ir_analyzer;
mod ir_converter;
pub(crate) mod lift_reversal;
mod loader;
#[cfg(test)]
mod tests;

pub use lift_reversal::reverse_generator_lifts_in_program;

// Re-export all public types
pub use core_ir_analyzer::{AnalysisResult, ConstantInfo, CoreIrAnalyzer, FunctionInfo};
pub use loader::{ir_file_to_aot_ir, load_ir_bytes, load_ir_file, program_to_aot_ir};

// Re-export for tests (IrConverter is pub(super) — only visible within analyze module)
#[cfg(test)]
pub(crate) use ir_converter::IrConverter;
