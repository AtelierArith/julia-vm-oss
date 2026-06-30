//! Issue #6538: cached-Base compile path must give the inference engine the
//! same view of multi-method Base callees as a fresh full compile.
//!
//! Before the fix, `build_method_tables` short-circuited cached Base functions
//! without registering their `MethodSig`s into the inference engine, and
//! `InferenceEngine::add_function` drops multi-signature names as ambiguous,
//! so a user function calling a multi-method Base function (without a
//! registered tfunc) inferred `Any` on the cached path while the uncached
//! path (`SUBSET_JULIA_VM_DISABLE_CACHE=1`) inferred precisely via the
//! method-table snapshot channel.
//!
//! These tests pin the parity end to end: the same source is compiled through
//! `compile_with_cache` (cached path) and `compile_core_program` (the exact
//! call the disabled-cache path makes), and `Base.infer_return_type` output
//! must agree — and match the precise expected types (verified against
//! upstream julia 1.12).

use subset_julia_vm::compile::{compile_core_program, compile_with_cache};
use subset_julia_vm::pipeline::parse_and_lower;
use subset_julia_vm::rng::StableRng;
use subset_julia_vm::vm::Vm;

/// Multi-method Base callees WITHOUT registered tfuncs (`mod1`, `factorial`,
/// `flipsign`) plus the original #6538 repro (`error`, now also covered by the
/// #6532 tfunc). Expected lines verified against upstream julia 1.12:
/// `Union{}`, `Int64`, `Int64`, `Int64`.
const INFERENCE_BATTERY_SOURCE: &str = r#"
b_err6538() = error("x")
b_mod1_6538(x::Int) = mod1(x, 7)
b_fact6538(n::Int) = factorial(n)
b_flipsign6538(x::Int, y::Int) = flipsign(x, y)
println(Base.infer_return_type(b_err6538, Tuple{}))
println(Base.infer_return_type(b_mod1_6538, Tuple{Int}))
println(Base.infer_return_type(b_fact6538, Tuple{Int}))
println(Base.infer_return_type(b_flipsign6538, Tuple{Int,Int}))
"#;

const EXPECTED_OUTPUT: &str = "Union{}\nInt64\nInt64\nInt64\n";

fn run_output(compiled: subset_julia_vm::vm::CompiledProgram) -> String {
    let mut vm = Vm::new_program(compiled, StableRng::new(42));
    vm.run().expect("VM execution failed");
    vm.get_output().to_string()
}

fn cached_path_output(src: &str) -> String {
    let program = parse_and_lower(src).expect("parse_and_lower failed");
    let compiled = compile_with_cache(&program).expect("cached compile failed");
    run_output(compiled)
}

fn uncached_path_output(src: &str) -> String {
    let program = parse_and_lower(src).expect("parse_and_lower failed");
    // Exactly what `compile_with_cache` does when
    // SUBSET_JULIA_VM_DISABLE_CACHE=1 is set (compile/cache.rs).
    let compiled = compile_core_program(&program).expect("uncached compile failed");
    run_output(compiled)
}

/// The structural pin: cached-path inference for multi-method Base callees
/// must match the uncached path (Issue #6538).
#[test]
fn cached_base_inference_matches_uncached_for_multi_method_callees_6538() {
    let cached = cached_path_output(INFERENCE_BATTERY_SOURCE);
    let uncached = uncached_path_output(INFERENCE_BATTERY_SOURCE);
    assert_eq!(
        cached, uncached,
        "cached-Base path inference diverged from the uncached path \
         (Issue #6538)\ncached:\n{cached}\nuncached:\n{uncached}"
    );
}

/// The precision pin: both paths must produce the upstream-verified precise
/// types, not a tfunc-registry `Any` fallback.
#[test]
fn cached_base_inference_is_precise_for_multi_method_callees_6538() {
    let cached = cached_path_output(INFERENCE_BATTERY_SOURCE);
    assert_eq!(
        cached, EXPECTED_OUTPUT,
        "cached-Base path must infer multi-method Base callees precisely \
         via the seeded method tables (Issue #6538)"
    );
}
