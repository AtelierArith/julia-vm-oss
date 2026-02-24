//! Bytecode analysis and IR conversion for AoT compilation.
//!
//! # Module Organization
//!
//! - `bytecode_analyzer.rs`: BytecodeAnalyzer for program analysis
//! - `ir_converter/`: IrConverter split by conversion responsibility (expr/stmt/helpers)
//! - `loader.rs`: Bytecode loading and conversion entry points
//! - `tests.rs`: Comprehensive test suite

mod bytecode_analyzer;
mod ir_converter;
mod loader;
#[cfg(test)]
mod tests;

// Re-export all public types
pub use bytecode_analyzer::{AnalysisResult, BytecodeAnalyzer, ConstantInfo, FunctionInfo};
pub use loader::{
    bytecode_file_to_aot_ir, load_bytecode_bytes, load_bytecode_file, program_to_aot_ir,
};

// Re-export for tests (IrConverter is pub(super) — only visible within analyze module)
#[cfg(test)]
pub(crate) use ir_converter::IrConverter;
