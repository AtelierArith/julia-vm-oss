//! Regression for Issue #9218: the iOS Editor OOM (~4 GB) on the Aizawa sample.
//!
//! The Editor runs a program through the FFI `compile_and_run_detailed`, which calls
//! `success_with_value` → `ffi_support::typed_value_json_string` to echo the result as
//! text. `gif(@animate ...)` returns a `Plots.AnimatedGif` whose `frames::Vector{Plot}`
//! hold the *cumulative* path in every frame — O(frames²) points (~1M for the stock
//! 9000-step / `every 40` Aizawa sample). Fully serializing that value (every point a
//! JSON node WITH its own `display` string) transiently allocated multiple GB and
//! OOM-killed the Editor. `typed_value_json` now bounds this: values whose capped leaf
//! estimate reaches `MAX_TYPED_VALUE_JSON_LEAVES` are echoed as a compact opaque
//! summary. The plot itself is unaffected — it still renders via the (bounded) display
//! artifact.

use subset_julia_vm::compile::host_support::compile_with_cache;
use subset_julia_vm::ffi_support::typed_value_json_string;
use subset_julia_vm::pipeline::parse_and_lower;
use subset_julia_vm::plotting::try_value_to_artifact;
use subset_julia_vm::rng::StableRng;
use subset_julia_vm::vm::Vm;
use subset_julia_vm_bytecode::Value;

fn run(src: &str) -> (Value, Vec<subset_julia_vm_bytecode::value::StructInstance>) {
    let program = parse_and_lower(src).expect("parse_and_lower");
    let compiled = compile_with_cache(&program).expect("compile");
    let mut vm = Vm::new_program(compiled, StableRng::new(0));
    let value = vm.run().expect("run");
    (value, vm.get_struct_heap().to_vec())
}

/// A large growing-path `@animate` result must NOT serialize its O(frames²) points
/// into the host result-echo JSON — it must be a compact opaque summary instead.
#[test]
fn editor_result_json_for_large_animation_is_bounded_9218() {
    // 300 frames, cumulative point count ≈ 4*(1+…+300) = 180_600 per axis (x and y),
    // well above the 100_000-leaf cap — but far cheaper to build than the 9000-step
    // stock sample, so the test stays fast.
    let src = r#"
using Plots
plt = plot(1, legend=false)
anim = @animate for i in 1:1200
    push!(plt, Float64(i), Float64(i))
end every 4
gif(anim)
"#;
    let (value, heap) = run(src);

    let json = typed_value_json_string(&value, &heap);
    assert!(
        json.len() < 4096,
        "large animation result echo must be bounded, got {} bytes",
        json.len()
    );
    assert!(
        json.contains("\"type\":\"opaque\"") && json.contains("\"reason\":\"too-large\""),
        "large result must be summarized as opaque/too-large, got: {json}"
    );

    // The plot itself is unaffected: it still renders via a compact display artifact.
    let artifact = try_value_to_artifact(&value, &heap)
        .expect("gif(anim) should still produce a display artifact");
    assert_eq!(artifact.mime, "application/vnd.plotly+json");
    assert!(
        artifact.data.contains("framesCompact"),
        "animation artifact should still use the compact growing-path schema"
    );
}

/// A small, ordinary result is unaffected: it is fully serialized (not opaque), so the
/// Editor keeps echoing normal values verbatim.
#[test]
fn editor_result_json_for_small_value_is_not_opaque_9218() {
    let (value, heap) = run("[1.0, 2.0, 3.0]");
    let json = typed_value_json_string(&value, &heap);
    assert!(
        !json.contains("\"too-large\""),
        "a 3-element array must serialize fully, not as opaque: {json}"
    );
    assert!(
        json.contains("\"type\":\"array\""),
        "small array should serialize as a typed array, got: {json}"
    );
}
