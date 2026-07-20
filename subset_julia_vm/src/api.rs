//! Rust API for compiling and running Julia code.
//!
//! This module provides ergonomic Rust functions for programmatic use.

#![deny(clippy::unwrap_used)]
#![deny(clippy::expect_used)]

use crate::cancel;
use crate::compile::host_support::compile_with_cache;
use crate::ir::core::Program;
use crate::pipeline::{
    parse_and_lower, parse_and_lower_with_base_dir_mode, PipelineError, SoftScopeMode,
};
use crate::rng::StableRng;
use crate::vm::Vm;
use subset_julia_vm_bytecode::value::StructInstance;
use subset_julia_vm_bytecode::Value;

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

/// Compile and run Julia subset source using **strict file-mode soft scope**
/// (Issue #9210), as the non-interactive CLI (`sjulia file.jl` / `-e`) does.
///
/// A top-level `for`/`while` body assignment to an existing global binds a NEW
/// local, so a read-before-write (`+=`) raises `UndefVarError` — matching
/// `julia file.jl`. [`compile_and_run_value`] keeps the lenient (REPL/host)
/// behaviour. Primarily used by regression tests for the file-vs-REPL split.
pub fn compile_and_run_value_file_mode(src: &str, seed: u64) -> Result<Value, String> {
    cancel::reset();

    let program = match parse_and_lower_with_base_dir_mode(src, None, SoftScopeMode::Strict, None) {
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

/// Compile and run Julia subset source under **strict file-mode soft scope**
/// (Issue #9210 / #9283), returning the result as f64 (NaN on error). This is
/// the `f64` analogue of [`compile_and_run_value_file_mode`] and the strict
/// counterpart of [`compile_and_run_str`]; the bundled-sample harnesses use it so
/// they validate the samples under the same soft scope the C ABI / WASM editor
/// hosts now apply.
pub fn compile_and_run_str_file_mode(src: &str, seed: u64) -> f64 {
    cancel::reset();

    let program = match parse_and_lower_with_base_dir_mode(src, None, SoftScopeMode::Strict, None) {
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
        Ok(value) => value_to_f64(&value, vm.get_struct_heap()),
        Err(_) => f64::NAN,
    }
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
        Ok(value) => value_to_f64(&value, vm.get_struct_heap()),
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
        Ok(value) => value_to_f64(&value, vm.get_struct_heap()),
        Err(_) => f64::NAN,
    }
}

fn value_to_f64(value: &Value, struct_heap: &[StructInstance]) -> f64 {
    match value {
        Value::I64(x) => *x as f64,
        Value::F64(x) => *x,
        Value::Bool(b) => {
            if *b {
                1.0
            } else {
                0.0
            }
        }
        Value::Nothing => -4.0,
        Value::Struct(s) => s.as_irrational_f64().unwrap_or(f64::NAN),
        Value::StructRef(idx) => struct_heap
            .get(*idx)
            .and_then(StructInstance::as_irrational_f64)
            .unwrap_or(f64::NAN),
        _ => f64::NAN,
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
    Ok(analyzer.analyze_program_with_production_inference(&program))
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
    let report = analyzer.analyze_program_with_production_inference(&program);

    format_json_report(&report)
}

#[cfg(test)]
#[allow(clippy::unwrap_used, clippy::expect_used)]
mod tests {
    use super::{analyze_type_stability, analyze_type_stability_json, compile_and_run_value};
    use crate::compile::lattice::types::{ConcreteType, LatticeType};
    use crate::inference_core::{CorePrimitive, CoreType};
    use subset_julia_vm_bytecode::{Instr, RuntimeNominalDefInfo, Value};

    include!("../tests/internal/runtime_nominal_template_11654_test.rs");

    #[test]
    fn type_stability_json_reports_inference_provenance() {
        let json = analyze_type_stability_json("f(x::Int64) = x + 1").expect("json report");

        assert!(json.contains("\"inference_provenance\""));
        assert!(json.contains("\"source\": \"production shared inference snapshot\""));
        assert!(json.contains("\"uses_production_inference\": true"));
    }

    #[test]
    fn type_stability_function_only_file_returns_quick_report_issue_4291() {
        let report = analyze_type_stability("plain_4291() = 41").expect("type-stability report");

        let function = report
            .functions
            .iter()
            .find(|function| function.function_name == "plain_4291")
            .expect("plain_4291 report");
        assert!(function.is_stable(), "{function:?}");
        assert_ne!(function.format_return_type(), "Any");
    }

    #[test]
    fn type_stability_preserves_user_struct_constructor_field_report_issue_4291() {
        let report = analyze_type_stability(
            r#"
struct TSBox4291
    x::Int64
end

make_box_4291() = TSBox4291(41)
field_from_box_4291() = make_box_4291().x
"#,
        )
        .expect("type-stability report");

        let make_box = report
            .functions
            .iter()
            .find(|function| function.function_name == "make_box_4291")
            .expect("make_box_4291 report");
        assert!(make_box.is_stable(), "{make_box:?}");
        // Issue #8544: the constructor return is now a first-class
        // `PartialStruct` fact; its widened type must still be the struct.
        assert!(
            matches!(
                &make_box.return_type.widen_partial_struct(),
                LatticeType::Concrete(ConcreteType::Struct { name, .. }) if name == "TSBox4291"
            ),
            "{make_box:?}"
        );

        let field = report
            .functions
            .iter()
            .find(|function| function.function_name == "field_from_box_4291")
            .expect("field_from_box_4291 report");
        assert!(field.is_stable(), "{field:?}");
        assert_ne!(field.format_return_type(), "Any");
    }

    #[test]
    fn type_stability_report_uses_source_line_not_byte_offset() {
        let report = analyze_type_stability(
            r#"

function f(x::Int64)
    return x + 1
end
"#,
        )
        .expect("type-stability report");

        let function = report
            .functions
            .iter()
            .find(|f| f.function_name == "f")
            .expect("f report");
        assert_eq!(function.line, 3);
    }

    #[test]
    fn type_stability_uses_global_types_for_const_reader() {
        let report = analyze_type_stability(
            r#"
const c4291 = 41
f4291() = c4291 + 1
"#,
        )
        .expect("type-stability report");

        let function = report
            .functions
            .iter()
            .find(|f| f.function_name == "f4291")
            .expect("f4291 report");
        assert!(function.is_stable(), "{function:?}");
        assert_eq!(function.format_return_type(), "Int64");
    }

    #[test]
    fn type_stability_report_matches_return_type_fixture_shapes_4291() {
        let src = r#"
using Test

function ts_array_4291(xs::Vector{Int64})
    map(x -> x + 1, xs)
end

ts_tuple_4291(t::Tuple{Int64,Float64}) = map(x -> x + 1, t)
ts_generator_4291(xs::Vector{Int64}) = collect(x + 1 for x in xs)

function ts_closure_generator_4291(a::Int64)
    f(x) = x + a
    collect(f(x) for x in 1:3)
end

ts_dispatch_4291(x::Integer) = 1
ts_dispatch_4291(x::Number) = 1.0
ts_dispatch_caller_4291(x::Int64) = ts_dispatch_4291(x)

struct TSBoxInner4291
    x::Int64
    TSBoxInner4291(x) = new(x + 1)
end

make_inner_4291() = TSBoxInner4291(40)
field_inner_4291() = make_inner_4291().x

@test Base.infer_return_type(ts_array_4291, Tuple{Vector{Int64}}) == Vector{Int64}
@test Base.infer_return_type(ts_tuple_4291, Tuple{Tuple{Int64,Float64}}) == Tuple{Int64,Float64}
@test Base.infer_return_type(ts_generator_4291, Tuple{Vector{Int64}}) == Vector{Int64}
@test Base.infer_return_type(ts_closure_generator_4291, Tuple{Int64}) == Vector{Int64}
@test Base.infer_return_type(ts_dispatch_caller_4291, Tuple{Int64}) === Int64

true
"#;

        assert!(matches!(
            compile_and_run_value(src, 0).expect("runtime/reflection fixture"),
            Value::Bool(true)
        ));

        let report = analyze_type_stability(src).expect("type-stability report");
        assert_report_array(
            &report,
            "ts_array_4291",
            ConcreteType::Core(CoreType::Primitive(CorePrimitive::Int64)),
            None,
        );
        assert_report_tuple(
            &report,
            "ts_tuple_4291",
            &[
                ConcreteType::Core(CoreType::Primitive(CorePrimitive::Int64)),
                ConcreteType::Core(CoreType::Primitive(CorePrimitive::Float64)),
            ],
        );
        assert_report_array(
            &report,
            "ts_generator_4291",
            ConcreteType::Core(CoreType::Primitive(CorePrimitive::Int64)),
            Some(1),
        );
        assert_report_array(
            &report,
            "ts_closure_generator_4291",
            ConcreteType::Core(CoreType::Primitive(CorePrimitive::Int64)),
            Some(1),
        );
        assert_report_concrete(
            &report,
            "ts_dispatch_caller_4291",
            ConcreteType::Core(CoreType::Primitive(CorePrimitive::Int64)),
        );
        assert_report_concrete_struct(&report, "make_inner_4291", "TSBoxInner4291");
        assert_report_concrete(
            &report,
            "field_inner_4291",
            ConcreteType::Core(CoreType::Primitive(CorePrimitive::Int64)),
        );
    }

    fn report_for<'a>(
        report: &'a crate::compile::type_stability::TypeStabilityAnalysisReport,
        name: &str,
    ) -> &'a crate::compile::type_stability::FunctionStabilityReport {
        report
            .functions
            .iter()
            .find(|function| function.function_name == name)
            .unwrap_or_else(|| panic!("{name} report should exist"))
    }

    fn assert_report_concrete(
        report: &crate::compile::type_stability::TypeStabilityAnalysisReport,
        name: &str,
        expected: ConcreteType,
    ) {
        let function = report_for(report, name);
        assert!(function.is_stable(), "{function:?}");
        assert_eq!(function.return_type, LatticeType::Concrete(expected));
    }

    fn assert_report_concrete_struct(
        report: &crate::compile::type_stability::TypeStabilityAnalysisReport,
        name: &str,
        expected_name: &str,
    ) {
        let function = report_for(report, name);
        assert!(function.is_stable(), "{function:?}");
        assert!(
            matches!(
                &function.return_type,
                LatticeType::Concrete(ConcreteType::Struct { name, .. }) if name == expected_name
            ),
            "{function:?}"
        );
    }

    fn assert_report_array(
        report: &crate::compile::type_stability::TypeStabilityAnalysisReport,
        name: &str,
        expected_element: ConcreteType,
        expected_ndims: Option<usize>,
    ) {
        let function = report_for(report, name);
        assert!(function.is_stable(), "{function:?}");
        assert_eq!(
            function.return_type,
            LatticeType::Concrete(ConcreteType::Array {
                element: Box::new(expected_element),
                ndims: expected_ndims
            })
        );
    }

    fn assert_report_tuple(
        report: &crate::compile::type_stability::TypeStabilityAnalysisReport,
        name: &str,
        expected_elements: &[ConcreteType],
    ) {
        let function = report_for(report, name);
        assert!(function.is_stable(), "{function:?}");
        assert_eq!(
            function.return_type,
            LatticeType::Concrete(ConcreteType::Tuple {
                elements: expected_elements.to_vec()
            })
        );
    }
}
