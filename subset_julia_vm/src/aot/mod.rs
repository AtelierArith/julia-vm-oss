//! AoT (Ahead-of-Time) Compiler Module
//!
//! This module provides the AoT compilation infrastructure for SubsetJuliaVM.
//! It transpiles Julia bytecode into Rust code for native execution.
//!
//! # Architecture
//!
//! ```text
//! Bytecode → Analyze → IR → Optimize → Codegen → Rust Code
//! ```
//!
//! # Compilation Levels
//!
//! The compiler supports different type inference levels:
//! - Level 0: Fully static types
//! - Level 1: Inferred types with guards
//! - Level 2: Conditional dispatch
//! - Level 3: Dynamic dispatch (fallback to runtime)

use thiserror::Error;

pub mod analyze;
pub mod call_graph;
pub mod codegen;
pub mod inference;
pub mod ir;
pub mod optimizer;
pub mod types;

/// AoT compilation error
#[derive(Debug, Error)]
pub enum AotError {
    /// Type inference failed
    #[error("Type inference failed: {0}")]
    TypeInferenceError(String),

    /// Unsupported bytecode instruction
    #[error("Unsupported instruction: {0}")]
    UnsupportedInstruction(String),

    /// Code generation error
    #[error("Code generation error: {0}")]
    CodegenError(String),

    /// Optimization error
    #[error("Optimization error: {0}")]
    OptimizationError(String),

    /// Invalid IR
    #[error("Invalid IR: {0}")]
    InvalidIR(String),

    /// Internal compiler error
    #[error("Internal compiler error: {0}")]
    InternalError(String),

    /// IR conversion error
    #[error("IR conversion error: {0}")]
    ConversionError(String),
}

/// Result type for AoT operations
pub type AotResult<T> = Result<T, AotError>;

/// Statistics collected during AoT compilation
#[derive(Debug, Default, Clone)]
pub struct AotStats {
    /// Number of functions compiled
    pub functions_compiled: usize,
    /// Total number of functions in program (before DCE)
    pub functions_total: usize,
    /// Number of functions eliminated by DCE
    pub functions_eliminated: usize,
    /// Number of instructions processed
    pub instructions_processed: usize,
    /// Number of type inferences performed
    pub type_inferences: usize,
    /// Number of dynamic dispatch fallbacks
    pub dynamic_fallbacks: usize,
    /// Number of optimizations applied
    pub optimizations_applied: usize,
}

impl AotStats {
    /// Create new empty statistics
    pub fn new() -> Self {
        Self::default()
    }

    /// Merge statistics from another compilation
    pub fn merge(&mut self, other: &AotStats) {
        self.functions_compiled += other.functions_compiled;
        self.functions_total += other.functions_total;
        self.functions_eliminated += other.functions_eliminated;
        self.instructions_processed += other.instructions_processed;
        self.type_inferences += other.type_inferences;
        self.dynamic_fallbacks += other.dynamic_fallbacks;
        self.optimizations_applied += other.optimizations_applied;
    }
}

/// Output from AoT compilation
#[derive(Debug)]
pub struct AotOutput {
    /// Generated Rust code
    pub rust_code: String,
    /// Compilation statistics
    pub stats: AotStats,
    /// Warnings generated during compilation
    pub warnings: Vec<String>,
}

impl AotOutput {
    /// Create a new AoT output
    pub fn new(rust_code: String, stats: AotStats) -> Self {
        Self {
            rust_code,
            stats,
            warnings: Vec::new(),
        }
    }

    /// Add a warning
    pub fn add_warning(&mut self, warning: String) {
        self.warnings.push(warning);
    }
}

/// Compile bytecode to Rust code
///
/// This is the main entry point for AoT compilation.
///
/// # Arguments
///
/// * `bytecode` - The bytecode to compile
///
/// # Returns
///
/// Returns `AotOutput` containing the generated Rust code and statistics.
pub fn compile_from_bytecode(_bytecode: &[u8]) -> AotResult<AotOutput> {
    // TODO(Issue #3132): Implement compile_from_bytecode in Phase 2
    Err(AotError::InternalError(
        "AoT compilation not yet implemented".to_string(),
    ))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_aot_stats_new() {
        let stats = AotStats::new();
        assert_eq!(stats.functions_compiled, 0);
        assert_eq!(stats.instructions_processed, 0);
    }

    #[test]
    fn test_aot_stats_merge() {
        let mut stats1 = AotStats {
            functions_compiled: 5,
            functions_total: 10,
            functions_eliminated: 5,
            instructions_processed: 100,
            type_inferences: 20,
            dynamic_fallbacks: 3,
            optimizations_applied: 10,
        };
        let stats2 = AotStats {
            functions_compiled: 3,
            functions_total: 6,
            functions_eliminated: 3,
            instructions_processed: 50,
            type_inferences: 10,
            dynamic_fallbacks: 1,
            optimizations_applied: 5,
        };
        stats1.merge(&stats2);
        assert_eq!(stats1.functions_compiled, 8);
        assert_eq!(stats1.functions_total, 16);
        assert_eq!(stats1.functions_eliminated, 8);
        assert_eq!(stats1.instructions_processed, 150);
        assert_eq!(stats1.type_inferences, 30);
        assert_eq!(stats1.dynamic_fallbacks, 4);
        assert_eq!(stats1.optimizations_applied, 15);
    }

    #[test]
    fn test_aot_output_new() {
        let stats = AotStats::new();
        let output = AotOutput::new("fn main() {}".to_string(), stats);
        assert_eq!(output.rust_code, "fn main() {}");
        assert!(output.warnings.is_empty());
    }

    #[test]
    fn test_aot_output_add_warning() {
        let stats = AotStats::new();
        let mut output = AotOutput::new(String::new(), stats);
        output.add_warning("unused variable".to_string());
        assert_eq!(output.warnings.len(), 1);
        assert_eq!(output.warnings[0], "unused variable");
    }

    #[test]
    fn test_compile_not_implemented() {
        let result = compile_from_bytecode(&[]);
        assert!(result.is_err());
    }
}
