//! Rust API for compiling and running Julia code.
//!
//! This module provides ergonomic Rust functions for programmatic use.

use crate::cancel;
use crate::compile::compile_with_cache;
use crate::ir::core::Program;
use crate::pipeline::{parse_and_lower, PipelineError};
use crate::rng::StableRng;
use crate::vm::{Value, Vm};

/// Compile and run Julia subset source (Rust string API).
/// Returns the result as f64. Returns NaN on error.
pub fn compile_and_run_str(src: &str, seed: u64) -> f64 {
    compile_and_run_auto_str(src, seed)
}

/// Compile and run Julia subset source, returning the actual Value.
/// This preserves type information (Bool, I64, F64, etc.).
pub fn compile_and_run_value(src: &str, seed: u64) -> Result<Value, String> {
    cancel::reset();

    let program = match parse_and_lower(src) {
        Ok(p) => p,
        Err(PipelineError::Parse(e)) => return Err(format!("parse error: {}", e)),
        Err(PipelineError::Lower(e)) => return Err(format!("lower error: {:?}", e)),
        Err(PipelineError::Load(e)) => return Err(format!("load error: {}", e)),
    };

    let compiled = match compile_with_cache(&program) {
        Ok(c) => c,
        Err(e) => return Err(format!("compile error: {:?}", e)),
    };

    let rng = StableRng::new(seed);
    let mut vm = Vm::new_program(compiled, rng);

    vm.run().map_err(|e| format!("runtime error: {}", e))
}

/// Compile and run using auto-detection (function or program).
/// Returns the result as f64. Returns NaN on error, -4.0 for Unit results.
pub fn compile_and_run_auto_str(src: &str, seed: u64) -> f64 {
    cancel::reset();

    let program = match parse_and_lower(src) {
        Ok(p) => p,
        Err(_) => return f64::NAN,
    };

    let compiled = match compile_with_cache(&program) {
        Ok(c) => c,
        Err(_) => return f64::NAN,
    };

    let rng = StableRng::new(seed);
    let mut vm = Vm::new_program(compiled, rng);

    match vm.run() {
        Ok(Value::I64(x)) => x as f64,
        Ok(Value::F64(x)) => x,
        Ok(Value::Bool(b)) => {
            if b {
                1.0
            } else {
                0.0
            }
        }
        Ok(Value::Nothing) => -4.0, // Special value for Unit
        Ok(_) => f64::NAN,
        Err(_) => f64::NAN,
    }
}

/// Compile Julia subset source to Core IR JSON (Rust string API).
pub fn compile_to_ir_str(src: &str) -> Option<String> {
    let program = match parse_and_lower(src) {
        Ok(p) => p,
        Err(_) => return None,
    };
    serde_json::to_string(&program).ok()
}

/// Run Core IR JSON (Rust string API).
pub fn run_ir_json_str(json: &str, _n: i64, seed: u64) -> f64 {
    let program: Program = match serde_json::from_str(json) {
        Ok(v) => v,
        Err(_) => return f64::NAN,
    };

    let compiled = match compile_with_cache(&program) {
        Ok(c) => c,
        Err(_) => return f64::NAN,
    };

    let rng = StableRng::new(seed);
    let mut vm = Vm::new_program(compiled, rng);

    match vm.run() {
        Ok(Value::I64(x)) => x as f64,
        Ok(Value::F64(x)) => x,
        Ok(_) => f64::NAN,
        Err(_) => f64::NAN,
    }
}

/// Analyze the type stability of functions in Julia source code.
///
/// Returns a `TypeStabilityAnalysisReport` containing:
/// - Summary statistics (total, stable, unstable counts)
/// - Detailed reports for each function
///
/// # Example
///
/// ```
/// use subset_julia_vm::api::analyze_type_stability;
///
/// let report = analyze_type_stability("f(x::Int64) = x * 2").unwrap();
/// assert!(report.all_stable());
/// ```
pub fn analyze_type_stability(
    src: &str,
) -> Result<crate::compile::type_stability::TypeStabilityAnalysisReport, String> {
    use crate::compile::type_stability::TypeStabilityAnalyzer;

    let program = match parse_and_lower(src) {
        Ok(p) => p,
        Err(PipelineError::Parse(e)) => return Err(format!("parse error: {}", e)),
        Err(PipelineError::Lower(e)) => return Err(format!("lower error: {:?}", e)),
        Err(PipelineError::Load(e)) => return Err(format!("load error: {}", e)),
    };

    let mut analyzer = TypeStabilityAnalyzer::new();
    Ok(analyzer.analyze_program(&program))
}

/// Analyze type stability and return the result as JSON string.
///
/// This is a convenience function that combines analysis and JSON serialization.
pub fn analyze_type_stability_json(src: &str) -> Result<String, String> {
    use crate::compile::type_stability::{format_json_report, TypeStabilityAnalyzer};

    let program = match parse_and_lower(src) {
        Ok(p) => p,
        Err(PipelineError::Parse(e)) => return Err(format!("parse error: {}", e)),
        Err(PipelineError::Lower(e)) => return Err(format!("lower error: {:?}", e)),
        Err(PipelineError::Load(e)) => return Err(format!("load error: {}", e)),
    };

    let mut analyzer = TypeStabilityAnalyzer::new();
    let report = analyzer.analyze_program(&program);

    format_json_report(&report)
}
