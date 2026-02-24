//! Shared helpers for integration tests
// This helper module is consumed selectively by many integration test files.
// Keep these utilities available without forcing every helper to be referenced
// in each individual test target.
#![allow(dead_code)]

use subset_julia_vm::compile::compile_core_program;
use subset_julia_vm::ir::core::Program;
use subset_julia_vm::lowering::Lowering;
use subset_julia_vm::parser::Parser;
use subset_julia_vm::rng::StableRng;
use subset_julia_vm::vm::{Value, Vm};
use subset_julia_vm::*;

/// Helper to run code through the Core IR pipeline (tree-sitter → lowering → compile_core)
/// This supports advanced features like struct definitions.
/// Includes prelude functions.
pub fn run_core_pipeline(src: &str, seed: u64) -> Result<Value, String> {
    // Use the same pipeline as compile_and_run_value for consistency
    // This properly merges with prelude and uses the compile cache
    compile_and_run_value(src, seed)
}

/// Helper to run code and return the VM output (println output)
/// Includes prelude functions. Uses the proper pipeline with caching.
pub fn compile_and_run_str_with_output(src: &str, seed: u64) -> String {
    use subset_julia_vm::compile::compile_with_cache;
    use subset_julia_vm::pipeline::parse_and_lower;

    // Use the proper pipeline that merges with prelude and uses compile cache
    let program = parse_and_lower(src)
        .unwrap_or_else(|e| panic!("Pipeline error: {}", e));

    let compiled = compile_with_cache(&program).expect("Compile failed");

    let rng = StableRng::new(seed);
    let mut vm = Vm::new_program(compiled, rng);
    let _ = vm.run();
    vm.get_output().to_string()
}

// Helper to compile and run using the tree-sitter pipeline (includes Base functions)
pub fn run_pipeline_with_output(src: &str, seed: u64) -> (Value, String) {
    use subset_julia_vm::base;

    // Parse Base source
    let prelude_src = base::get_base();
    let mut parser = Parser::new().expect("parser creation failed");
    let prelude_parsed = parser.parse(&prelude_src).expect("prelude parse failed");
    let mut prelude_lowering = Lowering::new(&prelude_src);
    let prelude_program = prelude_lowering
        .lower(prelude_parsed)
        .expect("prelude lowering failed");

    // Parse user source
    let mut parser = Parser::new().expect("parser creation failed");
    let parsed = parser.parse(src).expect("parse failed");
    let mut lowering = Lowering::new(src);
    let mut program = lowering.lower(parsed).expect("lowering failed");

    // Merge prelude functions, structs, and abstract types
    let mut all_functions = prelude_program.functions;
    all_functions.extend(program.functions);
    program.functions = all_functions;

    let mut all_structs = prelude_program.structs;
    all_structs.extend(program.structs);
    program.structs = all_structs;

    let mut all_abstract_types = prelude_program.abstract_types;
    all_abstract_types.extend(program.abstract_types);
    program.abstract_types = all_abstract_types;

    let compiled = compile_core_program(&program).expect("compile failed");
    let rng = StableRng::new(seed);
    let mut vm = Vm::new_program(compiled, rng);
    let result = vm.run().expect("VM execution failed");
    let output = vm.get_output().to_string();
    (result, output)
}

// Helper to compile and run a function directly (now uses tree-sitter pipeline)
pub fn compile_and_run_func(src: &str, _n: i64, seed: u64) -> Value {
    run_core_pipeline(src, seed).expect("pipeline failed")
}

// Helper to compile and run a program
pub fn compile_and_run_program_direct(src: &str, seed: u64) -> (Value, String) {
    run_pipeline_with_output(src, seed)
}

// Helper to compile and run a script (functions + main)
pub fn compile_and_run_script_direct(src: &str, seed: u64) -> (Value, String) {
    run_pipeline_with_output(src, seed)
}

/// Helper to run IR from JSON directly (simulates the web flow)
pub fn run_from_ir_json(ir_json: &str, seed: u64) -> Result<Value, String> {
    use std::collections::HashSet;
    use subset_julia_vm::base_loader::get_base_program;

    let mut program: Program =
        serde_json::from_str(ir_json).map_err(|e| format!("JSON parse error: {}", e))?;

    // Merge Base functions (operators, complex arithmetic, etc.)
    if let Some(base) = get_base_program() {
        // Collect user function names to avoid conflicts
        let user_func_names: HashSet<_> =
            program.functions.iter().map(|f| f.name.as_str()).collect();

        // Collect user struct names to avoid conflicts
        let user_struct_names: HashSet<_> =
            program.structs.iter().map(|s| s.name.as_str()).collect();

        // Merge structs (base first, but skip if user defines same name)
        let mut all_structs: Vec<_> = base
            .structs
            .iter()
            .filter(|&s| !user_struct_names.contains(s.name.as_str()))
            .cloned()
            .collect();
        all_structs.append(&mut program.structs);
        program.structs = all_structs;

        // Merge functions (base first, but skip if user defines same name)
        let mut all_functions: Vec<_> = base
            .functions
            .iter()
            .filter(|&f| !user_func_names.contains(f.name.as_str()))
            .cloned()
            .collect();
        all_functions.append(&mut program.functions);
        program.functions = all_functions;
    }

    let compiled = compile_core_program(&program).map_err(|e| format!("Compile error: {:?}", e))?;
    let rng = StableRng::new(seed);
    let mut vm = Vm::new_program(compiled, rng);
    vm.run().map_err(|e| format!("Runtime error: {}", e))
}

pub fn assert_ok_i64(result: Result<Value, String>, expected: i64) {
    let value = result.expect("Expected Ok result");
    assert_i64(value, expected);
}

pub fn assert_ok_f64(result: Result<Value, String>, expected: f64) {
    let value = result.expect("Expected Ok result");
    assert_f64(value, expected);
}

/// Assert that a Result<Value> is Ok and contains the expected numeric value.
/// Accepts either I64 or F64 (useful for pipeline tests that may return either).
pub fn assert_ok_numeric(result: Result<Value, String>, expected: f64) {
    let value = result.expect("Expected Ok result");
    match value {
        Value::I64(v) => assert_eq!(
            v, expected as i64,
            "Expected I64({}), got I64({})",
            expected as i64, v
        ),
        Value::F64(v) => assert!(
            (v - expected).abs() < 1e-10,
            "Expected F64({}), got F64({})",
            expected,
            v
        ),
        other => panic!("Expected numeric value ({}), got {:?}", expected, other),
    }
}

pub fn assert_i64(result: Value, expected: i64) {
    match result {
        Value::I64(v) => assert_eq!(v, expected, "Expected I64({}), got I64({})", expected, v),
        other => panic!("Expected I64({}), got {:?}", expected, other),
    }
}

pub fn assert_f64(result: Value, expected: f64) {
    match result {
        Value::F64(v) => assert!(
            (v - expected).abs() < 1e-10,
            "Expected F64({}), got F64({})",
            expected,
            v
        ),
        other => panic!("Expected F64({}), got {:?}", expected, other),
    }
}

pub fn assert_f32(result: Value, expected: f32) {
    match result {
        Value::F32(v) => assert!(
            (v - expected).abs() < 1e-5,
            "Expected F32({}), got F32({})",
            expected,
            v
        ),
        other => panic!("Expected F32({}), got {:?}", expected, other),
    }
}
