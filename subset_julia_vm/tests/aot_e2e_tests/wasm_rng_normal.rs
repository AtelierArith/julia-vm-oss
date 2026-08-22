use subset_julia_vm_bytecode::rng::{randn, RngLike, Xoshiro};

use super::support::{compile_rng_module, run_node};

#[test]
fn wasm_randn_matches_repository_xoshiro_streams() {
    let source = "normal64()::Float64 = randn()\nnormal32()::Float32 = randn()";
    let wasm = compile_rng_module(source, &[("normal64", vec![]), ("normal32", vec![])]);
    let mut oracle64 = Xoshiro::new(42);
    let expected64 = (0..1024)
        .map(|_| randn(&mut oracle64).to_bits().to_string())
        .collect::<Vec<_>>();
    assert!(expected64.iter().any(|bits| {
        f64::from_bits(bits.parse().expect("oracle bits are u64")).abs() > 3.654_152_885_361_009
    }));
    let mut oracle32 = Xoshiro::new(42);
    let expected32 = (0..1024)
        .map(|_| (randn(&mut oracle32) as f32).to_bits().to_string())
        .collect::<Vec<_>>()
        .join(",");
    let actual = run_node(
        &wasm,
        r#"
const sample = async (name, f32) => {
  const e = (await WebAssembly.instantiate(module, {})).exports;
  e.__sjulia_rng_seed(42n);
  const bits = f32
    ? x => new Uint32Array(new Float32Array([x]).buffer)[0].toString()
    : x => new BigUint64Array(new Float64Array([x]).buffer)[0].toString();
  return Array.from({length:1024}, () => bits(e[name]())).join(',');
};
console.log(JSON.stringify({
  imports: WebAssembly.Module.imports(module).length,
  normal64: await sample('normal64', false),
  normal32: await sample('normal32', true),
}));
"#,
    );
    let decoded: serde_json::Value = serde_json::from_str(&actual).expect("decode randn QA JSON");
    assert_eq!(decoded["imports"], 0);
    assert_eq!(decoded["normal64"], expected64.join(","));
    assert_eq!(decoded["normal32"], expected32);
}

#[test]
fn wasm_randn_preserves_seed_and_interleaved_stream_order() {
    let source = "uniform()::Float64 = rand()\nnormal()::Float64 = randn()";
    let wasm = compile_rng_module(source, &[("uniform", vec![]), ("normal", vec![])]);
    let seeds = [0_u64, u64::MAX, i64::MAX as u64];
    let expected_edges = seeds
        .iter()
        .map(|seed| {
            let mut oracle = Xoshiro::new(*seed);
            (0..16)
                .map(|_| randn(&mut oracle).to_bits().to_string())
                .collect::<Vec<_>>()
                .join(",")
        })
        .collect::<Vec<_>>();
    let mut interleaved_oracle = Xoshiro::new(42);
    let expected_interleaved = (0..64)
        .flat_map(|_| {
            [
                interleaved_oracle.next_f64().to_bits().to_string(),
                randn(&mut interleaved_oracle).to_bits().to_string(),
            ]
        })
        .collect::<Vec<_>>()
        .join(",");
    let actual = run_node(
        &wasm,
        r#"
const instantiate = () => WebAssembly.instantiate(module, {}).then(x => x.exports);
const bits = x => new BigUint64Array(new Float64Array([x]).buffer)[0].toString();
const a = await instantiate();
const b = await instantiate();
const edge = [0n, -1n, 9223372036854775807n].map(seed => {
  a.__sjulia_rng_seed(seed);
  return Array.from({length:16}, () => bits(a.normal())).join(',');
});
a.__sjulia_rng_seed(42n);
const first = Array.from({length:32}, () => bits(a.normal())).join(',');
a.__sjulia_rng_seed(42n);
const reseeded = Array.from({length:32}, () => bits(a.normal())).join(',');
a.__sjulia_rng_seed(42n);
b.__sjulia_rng_seed(43n);
const independentA = bits(a.normal());
const independentB = bits(b.normal());
a.__sjulia_rng_seed(42n);
const interleaved = Array.from({length:64}, () => [bits(a.uniform()), bits(a.normal())]).flat().join(',');
console.log(JSON.stringify({edge, first, reseeded, independentA, independentB, interleaved}));
"#,
    );
    let decoded: serde_json::Value = serde_json::from_str(&actual).expect("decode state QA JSON");
    assert_eq!(decoded["edge"], serde_json::json!(expected_edges));
    assert_eq!(decoded["first"], decoded["reseeded"]);
    assert_ne!(decoded["independentA"], decoded["independentB"]);
    assert_eq!(decoded["interleaved"], expected_interleaved);
}
