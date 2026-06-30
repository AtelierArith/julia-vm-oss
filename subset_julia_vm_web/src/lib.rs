// Prevent accidental debug output in library code (Issue #2888).
// CLI binaries (bin/) may use eprintln!() for user-facing error messages.
#![deny(clippy::print_stderr)]

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
    /// Structured typed representation of the returned VM value.
    pub typed_value: serde_json::Value,
    pub output: String,
    pub error_message: Option<String>,
    /// MIME type of the display artifact, currently always "application/vnd.plotly+json".
    pub artifact_mime: Option<String>,
    /// Artifact data string (Plotly JSON).
    pub artifact_data: Option<String>,
}

impl ExecutionResult {
    fn success(
        value: f64,
        typed_value: serde_json::Value,
        output: String,
        artifact: Option<(String, String)>,
    ) -> Self {
        let (artifact_mime, artifact_data) = artifact
            .map(|(m, d)| (Some(m), Some(d)))
            .unwrap_or((None, None));
        Self {
            success: true,
            value,
            typed_value,
            output,
            error_message: None,
            artifact_mime,
            artifact_data,
        }
    }

    fn error(message: String) -> Self {
        Self {
            success: false,
            value: f64::NAN,
            typed_value: serde_json::json!({
                "type": "error",
                "message": message.as_str(),
            }),
            output: String::new(),
            error_message: Some(message),
            artifact_mime: None,
            artifact_data: None,
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
    use subset_julia_vm::compile::compile_with_cache;
    use subset_julia_vm::ir::core::Program;
    use subset_julia_vm::rng::StableRng;
    use subset_julia_vm::vm::Vm;

    // Parse IR JSON
    let program: Program = match serde_json::from_str(ir_json) {
        Ok(p) => p,
        Err(e) => return ExecutionResult::error(format!("IR JSON parse error: {}", e)),
    };

    // Compile through the cache path so first-run work is amortized in the Web
    // Playground and repeated samples reuse compiled Base/programs (Issue #6022).
    let compiled = match compile_with_cache(&program) {
        Ok(c) => c,
        Err(e) => return ExecutionResult::error(format!("Compile error: {:?}", e)),
    };

    // Execute
    let rng = StableRng::new(seed);
    let mut vm = Vm::new_program(compiled, rng);

    match vm.run() {
        Ok(value) => {
            let output = vm.get_output().to_string();
            let f64_value = subset_julia_vm::ffi_support::legacy_numeric_result_value(&value);
            let typed_value =
                subset_julia_vm::ffi_support::typed_value_json(&value, vm.get_struct_heap());
            // Plot artifact extraction is not supported for raw IR execution — this entry
            // point is a numeric runner used for pre-compiled programs. The Web Playground
            // uses run_from_source_internal, which does extract plot artifacts.
            ExecutionResult::success(f64_value, typed_value, output, None)
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

/// Run Julia source code and return an ExecutionResult with `typed_value`
/// populated as a structured JavaScript object.
#[wasm_bindgen]
pub fn run_from_source_typed(source: &str, seed: u64) -> JsValue {
    run_from_source(source, seed)
}

fn run_from_source_internal(source: &str, seed: u64) -> ExecutionResult {
    use subset_julia_vm::compile::compile_with_cache;
    use subset_julia_vm::pipeline::parse_and_lower;
    use subset_julia_vm::rng::StableRng;
    use subset_julia_vm::vm::Vm;

    // Parse, lower, merge prelude, and resolve `using` imports (stdlib + bundled
    // packages such as Primes/Plots/Example via PackageLoader). Going through the
    // shared pipeline keeps the WASM entry point in lockstep with the CLI/iOS
    // paths so bundled-package functions are available in the Web Playground
    // (Issue #4373).
    let program = match parse_and_lower(source) {
        Ok(p) => p,
        Err(e) => return ExecutionResult::error(format!("{}", e)),
    };

    // Compile through the cache path so first-run work is amortized in the Web
    // Playground and repeated samples reuse compiled Base/programs (Issue #6022).
    let compiled = match compile_with_cache(&program) {
        Ok(c) => c,
        Err(e) => return ExecutionResult::error(format!("Compile error: {:?}", e)),
    };

    // Execute
    let rng = StableRng::new(seed);
    let mut vm = Vm::new_program(compiled, rng);

    match vm.run() {
        Ok(value) => {
            let output = vm.get_output().to_string();
            let f64_value = subset_julia_vm::ffi_support::legacy_numeric_result_value(&value);
            let typed_value =
                subset_julia_vm::ffi_support::typed_value_json(&value, vm.get_struct_heap());
            let artifact =
                subset_julia_vm::plotting::try_value_to_artifact(&value, vm.get_struct_heap())
                    .map(|a| (a.mime, a.data));
            ExecutionResult::success(f64_value, typed_value, output, artifact)
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
        "using (Statistics, Test, Random, Dates, Plots)",
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
    let features = unsupported_features();
    serde_wasm_bindgen::to_value(&features).unwrap_or(JsValue::NULL)
}

fn unsupported_features() -> Vec<&'static str> {
    vec!["eval() parity gaps", "@generated", "native C extensions"]
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

    #[test]
    fn test_bundled_primes_factor() {
        // Regression for Issue #4373: `using Primes; factor(10)` must work in the
        // WASM entry point, not just the CLI/iOS pipelines.
        let source = "using Primes\nprintln(factor(10))\n";
        let result = run_from_source_internal(source, 42);
        assert!(
            result.success,
            "expected success, got error: {:?}",
            result.error_message
        );
        // Issue #7171: Factorization prints in upstream `p1 ⋅ p2` form via the
        // package's `Base.show`, not the raw struct field dump.
        assert!(
            result.output.contains("2 ⋅ 5"),
            "unexpected output: {:?}",
            result.output
        );
    }

    #[test]
    fn test_bundled_primes_isprime() {
        // Companion check: another export from the bundled Primes package
        // is reachable from the WASM entry point (Issue #4373). We only
        // assert that `isprime` resolves and runs — the Bool printing
        // quirk (Int 1 vs `true`) is a separate, pre-existing issue.
        let source = "using Primes\nprintln(isprime(17))\n";
        let result = run_from_source_internal(source, 42);
        assert!(
            result.success,
            "expected success, got error: {:?}",
            result.error_message
        );
    }

    #[test]
    fn test_run_from_source_plot_returns_plotly() {
        // Issue #5283: 2D plots render through Plotly just like 3D, so
        // run_from_source_internal must surface a Plotly JSON artifact.
        let source = "using Plots\nplot(sin)\n";
        let result = run_from_source_internal(source, 42);
        assert!(
            result.success,
            "expected success, got error: {:?}",
            result.error_message
        );
        assert_eq!(
            result.artifact_mime.as_deref(),
            Some("application/vnd.plotly+json"),
            "artifact_mime should be Plotly JSON for a 2D plot"
        );
        let data = result
            .artifact_data
            .as_deref()
            .expect("expected Plotly artifact data for plot(sin)");
        assert!(
            data.contains(r#""type":"scatter""#) && data.contains(r#""mode":"lines""#),
            "2D line plot should be a scatter/lines trace, got: {:.200}",
            data
        );
    }

    #[test]
    fn test_run_from_source_typed_array_value_8456() {
        let result = run_from_source_internal("[1.0, 2.5, 3.0]\n", 42);
        assert!(
            result.success,
            "expected success, got error: {:?}",
            result.error_message
        );
        assert_eq!(result.typed_value["type"], "array");
        assert_eq!(result.typed_value["element_type"], "Float64");
        assert_eq!(result.typed_value["shape"], serde_json::json!([3]));
        assert_eq!(result.typed_value["elements"][0]["value"], 1.0);
        assert_eq!(result.typed_value["elements"][1]["value"], 2.5);
        assert_eq!(result.typed_value["elements"][2]["value"], 3.0);
    }

    #[test]
    fn test_run_from_source_typed_complex_value_8456() {
        let result = run_from_source_internal("complex(1.5, 2.25)\n", 42);
        assert!(
            result.success,
            "expected success, got error: {:?}",
            result.error_message
        );
        assert_eq!(result.typed_value["type"], "complex");
        assert_eq!(result.typed_value["real"], 1.5);
        assert_eq!(result.typed_value["imag"], 2.25);
    }

    #[test]
    fn test_run_from_source_typed_plot_artifact_8456() {
        let result = run_from_source_internal("using Plots\nplot(sin)\n", 42);
        assert!(
            result.success,
            "expected success, got error: {:?}",
            result.error_message
        );
        assert_eq!(
            result.artifact_mime.as_deref(),
            Some("application/vnd.plotly+json")
        );
        let data = result
            .artifact_data
            .as_deref()
            .expect("expected Plotly artifact data");
        let parsed: serde_json::Value =
            serde_json::from_str(data).expect("artifact_data should be raw Plotly JSON");
        assert!(
            parsed.to_string().contains(r#""scatter""#),
            "Plotly artifact should contain a scatter trace, got: {:.200}",
            data
        );
    }

    #[test]
    fn test_unsupported_features_does_not_list_macro_definitions_8456() {
        let unsupported = unsupported_features();
        assert!(
            unsupported
                .iter()
                .all(|feature| !feature.contains("macro definitions")),
            "user-defined macro definitions are implemented and should not be listed"
        );
    }

    #[test]
    fn test_run_from_source_initializes_compile_cache_6022() {
        subset_julia_vm::compile::cache::clear_cache();
        assert!(
            !subset_julia_vm::compile::cache::is_cache_initialized(),
            "cache should start empty for this test"
        );

        let result = run_from_source_internal("1 + 1\n", 42);
        assert!(
            result.success,
            "expected success, got error: {:?}",
            result.error_message
        );
        assert!(
            subset_julia_vm::compile::cache::is_cache_initialized(),
            "web run_from_source should initialize compile cache (Issue #6022)"
        );

        subset_julia_vm::compile::cache::clear_cache();
    }

    #[test]
    fn test_run_from_source_plot3d_returns_plotly() {
        let source =
            "using Plots\nxs=[0.0,1.0,2.0]\nys=[0.0,1.0,2.0]\nzs=[0.0,1.0,4.0]\nplot(xs,ys,zs)\n";
        let result = run_from_source_internal(source, 42);
        assert!(
            result.success,
            "expected success, got error: {:?}",
            result.error_message
        );
        assert_eq!(
            result.artifact_mime.as_deref(),
            Some("application/vnd.plotly+json"),
            "artifact_mime should be application/vnd.plotly+json for 3D plot"
        );
        let data = result
            .artifact_data
            .as_deref()
            .expect("expected artifact_data for 3D plot");
        assert!(
            data.contains("scatter3d"),
            "Plotly JSON should contain scatter3d trace type, got: {:.200}",
            data
        );
    }

    #[test]
    fn test_run_from_source_surface_returns_plotly() {
        let source =
            "using Plots\nxs=[0.0,1.0]\nys=[0.0,1.0]\nz=[0.0 1.0; 2.0 3.0]\nsurface(xs,ys,z)\n";
        let result = run_from_source_internal(source, 42);
        assert!(
            result.success,
            "expected success, got error: {:?}",
            result.error_message
        );
        assert_eq!(
            result.artifact_mime.as_deref(),
            Some("application/vnd.plotly+json"),
        );
        let data = result
            .artifact_data
            .as_deref()
            .expect("expected artifact_data");
        assert!(data.contains("\"type\":\"surface\""));
    }

    // ===== Web Playground sample parity (Issue #7286) =====
    //
    // These exercise the exact iOS-app sample sources through the WASM entry point
    // (`run_from_source_internal`), proving that the bundled packages they need
    // (Primes / Symbolics / Distributions) and their display artifacts resolve in
    // the static web build — the same code path the Web Playground runs. The
    // bundled packages are `include_str!`-embedded into `subset_julia_vm`, so they
    // ship with the WASM binary; there is no separate base-cache file to populate.
    // Once these pass, the corresponding `web/samples_ir.js` entries can drop their
    // `webUnsupported: true` flag.

    const IFS_FRACTALS_JL: &str = include_str!(
        "../../SubsetJuliaVMApp/SubsetJuliaVMApp/Resources/Samples/intermediate/ifs_fractals.jl"
    );
    const JSXGRAPH_DEMO_JL: &str = include_str!(
        "../../SubsetJuliaVMApp/SubsetJuliaVMApp/Resources/Samples/intermediate/jsxgraph_demo.jl"
    );
    const JSXGRAPH_LISSAJOUS_3D_JL: &str = include_str!(
        "../../SubsetJuliaVMApp/SubsetJuliaVMApp/Resources/Samples/intermediate/jsxgraph_lissajous_3d.jl"
    );
    const APOLLONIAN_GASKET_JL: &str = include_str!(
        "../../SubsetJuliaVMApp/SubsetJuliaVMApp/Resources/Samples/intermediate/apollonian_gasket.jl"
    );
    const PRIMES_PACKAGE_JL: &str = include_str!(
        "../../SubsetJuliaVMApp/SubsetJuliaVMApp/Resources/Samples/advanced/primes_package.jl"
    );
    const SYMBOLICS_PACKAGE_JL: &str = include_str!(
        "../../SubsetJuliaVMApp/SubsetJuliaVMApp/Resources/Samples/advanced/symbolics_package.jl"
    );
    const SYMBOLICS_LINEAR_ALGEBRA_JL: &str = include_str!(
        "../../SubsetJuliaVMApp/SubsetJuliaVMApp/Resources/Samples/advanced/symbolics_linear_algebra.jl"
    );
    const DISTRIBUTIONS_PACKAGE_JL: &str = include_str!(
        "../../SubsetJuliaVMApp/SubsetJuliaVMApp/Resources/Samples/advanced/distributions_package.jl"
    );

    #[test]
    fn test_web_sample_primes_package_runs() {
        let result = run_from_source_internal(PRIMES_PACKAGE_JL, 42);
        assert!(
            result.success,
            "primes_package should run in the web build: {:?}",
            result.error_message
        );
        // factor(360) = 2^3 ⋅ 3^2 ⋅ 5 via the package's Base.show.
        assert!(
            result.output.contains("2^3 ⋅ 3^2 ⋅ 5"),
            "unexpected output: {:?}",
            result.output
        );
        assert!(result.output.contains("totient(36)  = 12"));
    }

    #[test]
    fn test_web_sample_symbolics_package_runs() {
        let result = run_from_source_internal(SYMBOLICS_PACKAGE_JL, 42);
        assert!(
            result.success,
            "symbolics_package should run in the web build: {:?}",
            result.error_message
        );
        assert!(
            result.output.contains("Differential cos   = -sin(x)"),
            "unexpected output: {:?}",
            result.output
        );
    }

    #[test]
    fn test_web_sample_symbolics_linear_algebra_runs() {
        let result = run_from_source_internal(SYMBOLICS_LINEAR_ALGEBRA_JL, 42);
        assert!(
            result.success,
            "symbolics_linear_algebra should run in the web build: {:?}",
            result.error_message
        );
        // det renders in upstream-identical canonical form (Issue #7894).
        assert!(
            result.output.contains("det(A)         = x^2 - x*y"),
            "unexpected output: {:?}",
            result.output
        );
    }

    #[test]
    fn test_web_sample_distributions_package_runs() {
        // The bundled Distributions package resolves in the web build path and the
        // full sample runs on the host target (Normal stats/sampling/fit + Binomial
        // pdf/cdf). NOTE: in the wasm32 target only, `cdf(::Binomial, …)` currently
        // fails — it routes through SpecialFunctions' `_beta_inc_cf`, whose trailing
        // default arguments do not dispatch under wasm32 (works natively / in the
        // CLI). That is why `distributions_package` stays `webUnsupported: true` in
        // `web/samples_ir.js`; this host-side test still guards that the package and
        // the Normal path keep working end-to-end (Issue #7286).
        let result = run_from_source_internal(DISTRIBUTIONS_PACKAGE_JL, 42);
        assert!(
            result.success,
            "distributions_package should run in the (native) web build path: {:?}",
            result.error_message
        );
        // The bundled sample no longer prints a `Distribution: ...` line; assert
        // stable lines from the current sample instead (Issue #7824): the Normal
        // stats header, the pdf API line, and the closing fit_mle summary so the
        // assertion still covers the whole Normal path end-to-end.
        assert!(
            result.output.contains("Normal mean/std = 2.0 / 3.0"),
            "unexpected output: {:?}",
            result.output
        );
        assert!(
            result
                .output
                .contains("pdf(Normal, 2)  = 0.13298076013381094"),
            "unexpected output: {:?}",
            result.output
        );
        assert!(
            result
                .output
                .contains("fit_mle mean/std = 3.0 / 1.4142135623730951"),
            "unexpected output: {:?}",
            result.output
        );
    }

    #[test]
    fn test_web_sample_ifs_fractals_returns_plotly() {
        // ifs_fractals uses Interact.@manipulate over Distributions.Categorical +
        // Plots.scatter; it renders through Plotly (a dropdown figure), which the
        // web playground already displays.
        let result = run_from_source_internal(IFS_FRACTALS_JL, 42);
        assert!(
            result.success,
            "ifs_fractals should run in the web build: {:?}",
            result.error_message
        );
        assert_eq!(
            result.artifact_mime.as_deref(),
            Some("application/vnd.plotly+json"),
            "ifs_fractals should produce a Plotly artifact"
        );
    }

    #[test]
    fn test_web_sample_jsxgraph_demo_returns_jsxgraph_artifact() {
        let result = run_from_source_internal(JSXGRAPH_DEMO_JL, 42);
        assert!(
            result.success,
            "jsxgraph_demo should run in the web build: {:?}",
            result.error_message
        );
        assert_eq!(
            result.artifact_mime.as_deref(),
            Some("application/vnd.jsxgraph+json"),
            "jsxgraph_demo should produce a JSXGraph artifact"
        );
        let data = result
            .artifact_data
            .as_deref()
            .expect("expected JSXGraph artifact data");
        // The artifact carries board options plus an ordered element list.
        assert!(
            data.contains("\"options\"") && data.contains("\"elements\""),
            "unexpected JSXGraph JSON: {:.200}",
            data
        );
    }

    #[test]
    fn test_web_sample_apollonian_gasket_returns_jsxgraph_artifact() {
        let result = run_from_source_internal(APOLLONIAN_GASKET_JL, 42);
        assert!(
            result.success,
            "apollonian_gasket should run in the web build: {:?}",
            result.error_message
        );
        assert_eq!(
            result.artifact_mime.as_deref(),
            Some("application/vnd.jsxgraph+json"),
            "apollonian_gasket should produce a JSXGraph artifact"
        );
    }

    #[test]
    fn test_web_sample_jsxgraph_lissajous_3d_returns_nested_jsxgraph_artifact() {
        let result = run_from_source_internal(JSXGRAPH_LISSAJOUS_3D_JL, 42);
        assert!(
            result.success,
            "jsxgraph_lissajous_3d should run in the web build: {:?}",
            result.error_message
        );
        assert_eq!(
            result.artifact_mime.as_deref(),
            Some("application/vnd.jsxgraph+json"),
            "jsxgraph_lissajous_3d should produce a JSXGraph artifact"
        );
        let data = result
            .artifact_data
            .as_deref()
            .expect("expected JSXGraph artifact data");
        assert!(
            data.contains("\"type\":\"view3d\"") && data.contains("\"jsfunc\""),
            "unexpected JSXGraph 3D JSON: {:.300}",
            data
        );
    }
}
