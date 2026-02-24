//! WebAssembly bindings for SubsetJuliaVM
//!
//! This crate provides WASM bindings for the SubsetJuliaVM Julia subset interpreter.
//!
//! Usage:
//! - Call `run_from_source(julia_code, seed)` to execute Julia code
//! - Uses the pure Rust parser (subset_julia_vm_parser) that works natively in WASM

use serde::{Deserialize, Serialize};
use wasm_bindgen::prelude::*;

// Set up panic hook for better error messages in browser console
#[wasm_bindgen(start)]
pub fn init() {
    console_error_panic_hook::set_once();
}

/// Execution result returned to JavaScript
#[derive(Serialize, Deserialize)]
pub struct ExecutionResult {
    pub success: bool,
    pub value: f64,
    pub output: String,
    pub error_message: Option<String>,
}

impl ExecutionResult {
    fn success(value: f64, output: String) -> Self {
        Self {
            success: true,
            value,
            output,
            error_message: None,
        }
    }

    fn error(message: String) -> Self {
        Self {
            success: false,
            value: f64::NAN,
            output: String::new(),
            error_message: Some(message),
        }
    }
}

/// Run a Core IR JSON program and return the result.
///
/// This function takes a JSON-serialized Core IR program and executes it.
/// The IR should be generated from Julia source code using the lowering pipeline.
///
/// # Arguments
/// * `ir_json` - JSON string representing the Core IR program
/// * `seed` - Random seed for deterministic execution
///
/// # Returns
/// An ExecutionResult object containing success status, value, output, and error message
#[wasm_bindgen]
pub fn run_ir_json(ir_json: &str, seed: u64) -> JsValue {
    let result = run_ir_internal(ir_json, seed);
    serde_wasm_bindgen::to_value(&result).unwrap_or(JsValue::NULL)
}

fn run_ir_internal(ir_json: &str, seed: u64) -> ExecutionResult {
    use subset_julia_vm::compile::compile_core_program;
    use subset_julia_vm::ir::core::Program;
    use subset_julia_vm::rng::StableRng;
    use subset_julia_vm::vm::{Value, Vm};

    // Parse IR JSON
    let program: Program = match serde_json::from_str(ir_json) {
        Ok(p) => p,
        Err(e) => return ExecutionResult::error(format!("IR JSON parse error: {}", e)),
    };

    // Compile (compile_core_program automatically merges with precompiled base)
    let compiled = match compile_core_program(&program) {
        Ok(c) => c,
        Err(e) => return ExecutionResult::error(format!("Compile error: {:?}", e)),
    };

    // Execute
    let rng = StableRng::new(seed);
    let mut vm = Vm::new_program(compiled, rng);

    match vm.run() {
        Ok(value) => {
            let output = vm.get_output().to_string();
            let f64_value = match &value {
                Value::I64(x) => *x as f64,
                Value::F64(x) => *x,
                Value::Nothing => 0.0, // nothing returns 0.0 for numeric context
                v if v.is_complex() => v.as_complex_parts().map(|(re, _)| re).unwrap_or(f64::NAN),
                _ => f64::NAN,
            };
            ExecutionResult::success(f64_value, output)
        }
        Err(e) => ExecutionResult::error(format!("Runtime error: {}", e)),
    }
}

/// Run IR JSON and return just the numeric result.
/// Returns NaN on error.
#[wasm_bindgen]
pub fn run_ir_simple(ir_json: &str, seed: u64) -> f64 {
    let result = run_ir_internal(ir_json, seed);
    if result.success {
        result.value
    } else {
        f64::NAN
    }
}

/// Run Julia source code directly using the pure Rust parser.
///
/// This is the recommended entry point for running Julia code in WASM.
/// It uses subset_julia_vm_parser which is a pure Rust parser that works
/// natively in WASM without requiring web-tree-sitter.
///
/// # Arguments
/// * `source` - Julia source code to execute
/// * `seed` - Random seed for deterministic execution
///
/// # Returns
/// An ExecutionResult object containing success status, value, output, and error message
#[wasm_bindgen]
pub fn run_from_source(source: &str, seed: u64) -> JsValue {
    let result = run_from_source_internal(source, seed);
    serde_wasm_bindgen::to_value(&result).unwrap_or(JsValue::NULL)
}

fn run_from_source_internal(source: &str, seed: u64) -> ExecutionResult {
    use subset_julia_vm::compile::compile_core_program;
    use subset_julia_vm::lowering::Lowering;
    use subset_julia_vm::parser::{ParseOutcome, RustParsedSource};
    use subset_julia_vm::rng::StableRng;
    use subset_julia_vm::vm::{Value, Vm};

    // Parse using the pure Rust parser
    let cst = match subset_julia_vm_parser::parse(source) {
        Ok(cst) => cst,
        Err(e) => return ExecutionResult::error(format!("Parse error: {}", e)),
    };

    // Create ParseOutcome for the full Lowering pipeline
    let parse_outcome = ParseOutcome::Rust(RustParsedSource {
        cst,
        source: source.to_string(),
    });

    // Lower CST to IR using the full Lowering (supports all features including keyword_argument)
    let mut lowering = Lowering::new(source);
    let program = match lowering.lower(parse_outcome) {
        Ok(p) => p,
        Err(e) => return ExecutionResult::error(format!("Lowering error: {}", e)),
    };

    // Compile (automatically merges with Base)
    let compiled = match compile_core_program(&program) {
        Ok(c) => c,
        Err(e) => return ExecutionResult::error(format!("Compile error: {:?}", e)),
    };

    // Execute
    let rng = StableRng::new(seed);
    let mut vm = Vm::new_program(compiled, rng);

    match vm.run() {
        Ok(value) => {
            let output = vm.get_output().to_string();
            let f64_value = match &value {
                Value::I64(x) => *x as f64,
                Value::F64(x) => *x,
                Value::Nothing => 0.0,
                v if v.is_complex() => v.as_complex_parts().map(|(re, _)| re).unwrap_or(f64::NAN),
                _ => f64::NAN,
            };
            ExecutionResult::success(f64_value, output)
        }
        Err(e) => ExecutionResult::error(format!("Runtime error: {}", e)),
    }
}

/// Get the version of the VM
#[wasm_bindgen]
pub fn get_version() -> String {
    env!("CARGO_PKG_VERSION").to_string()
}

/// List of supported Julia subset features
#[wasm_bindgen]
pub fn get_supported_features() -> JsValue {
    let features = vec![
        "functions",
        "loops (for, while)",
        "conditionals (if/else)",
        "arrays (1D, 2D)",
        "complex numbers",
        "structs",
        "modules",
        "using (Statistics, Test, Random, Dates)",
        "try/catch/finally",
        "lambdas",
        "higher-order functions (map, filter, reduce)",
        "broadcast operations (.*. .+)",
        "random numbers (rand)",
        "math functions (sin, cos, sqrt, etc.)",
    ];
    serde_wasm_bindgen::to_value(&features).unwrap_or(JsValue::NULL)
}

/// List of unsupported features
#[wasm_bindgen]
pub fn get_unsupported_features() -> JsValue {
    let features = vec!["macro definitions", "eval()", "@generated", "C extensions"];
    serde_wasm_bindgen::to_value(&features).unwrap_or(JsValue::NULL)
}

// ===== Unicode Completion API =====

/// Look up a LaTeX command and return its Unicode representation.
/// Returns null if not found.
#[wasm_bindgen]
pub fn unicode_lookup(latex: &str) -> Option<String> {
    subset_julia_vm::unicode::latex_to_unicode(latex).map(|s| s.to_string())
}

/// Get completions for a LaTeX prefix.
/// Returns a JSON array of [latex, unicode] pairs.
#[wasm_bindgen]
pub fn unicode_completions(prefix: &str) -> JsValue {
    let completions = subset_julia_vm::unicode::completions_for_prefix(prefix);
    let pairs: Vec<(&str, &str)> = completions.into_iter().collect();
    serde_wasm_bindgen::to_value(&pairs).unwrap_or(JsValue::NULL)
}

/// Expand all LaTeX sequences in a string to their Unicode equivalents.
#[wasm_bindgen]
pub fn unicode_expand(input: &str) -> String {
    subset_julia_vm::unicode::expand_latex_in_string(input)
}

/// Reverse lookup: get LaTeX for a Unicode character.
/// Returns null if not found.
#[wasm_bindgen]
pub fn unicode_reverse_lookup(unicode_char: &str) -> Option<String> {
    subset_julia_vm::unicode::unicode_to_latex(unicode_char).map(|s| s.to_string())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_version() {
        assert!(!get_version().is_empty());
    }
}
