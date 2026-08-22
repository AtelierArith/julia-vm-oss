use subset_julia_vm_bytecode::rng::{RngLike, Xoshiro};

use super::support::{compile_rng_module, run_node};

#[test]
fn wasm_rng_matches_xoshiro_uniform_streams() {
    let source = "uniform()::Float64 = rand()";
    let wasm = compile_rng_module(source, &[("uniform", vec![])]);
    let mut uniform_oracle = Xoshiro::new(42);
    let expected_uniform = (0..1024)
        .map(|_| uniform_oracle.next_f64().to_bits().to_string())
        .collect::<Vec<_>>()
        .join(",");
    let actual = run_node(
        &wasm,
        r#"
const sample = (name, seed) => WebAssembly.instantiate(module, {}).then(({exports:e}) => {
  e.__sjulia_rng_seed(seed);
  return Array.from({length:1024}, () => e[name]()).map(x => new BigUint64Array(new Float64Array([x]).buffer)[0].toString()).join(',');
});
const [uniformA, uniformB] = await Promise.all([sample('uniform', 42n), sample('uniform', 42n)]);
console.log(JSON.stringify({imports:WebAssembly.Module.imports(module).length, uniformA, uniformB}));
"#,
    );
    let decoded: serde_json::Value = serde_json::from_str(&actual).expect("decode RNG QA JSON");
    assert_eq!(decoded["imports"], 0);
    assert_eq!(decoded["uniformA"], expected_uniform);
    assert_eq!(decoded["uniformB"], expected_uniform);
}

#[test]
fn wasm_rng_reseeds_edge_seeds_and_rounds_float32() {
    let source = "uniform64()::Float64 = rand()\nuniform32()::Float32 = rand()";
    let wasm = compile_rng_module(source, &[("uniform64", vec![]), ("uniform32", vec![])]);
    let seeds = [0_u64, u64::MAX, i64::MAX as u64];
    let expected64 = seeds
        .iter()
        .map(|seed| {
            let mut oracle = Xoshiro::new(*seed);
            (0..8)
                .map(|_| oracle.next_f64().to_bits().to_string())
                .collect::<Vec<_>>()
                .join(",")
        })
        .collect::<Vec<_>>();
    let mut float32_oracle = Xoshiro::new(42);
    let expected32 = (0..1024)
        .map(|_| (float32_oracle.next_f64() as f32).to_bits().to_string())
        .collect::<Vec<_>>()
        .join(",");
    let actual = run_node(
        &wasm,
        r#"
const instantiate = () => WebAssembly.instantiate(module, {}).then(result => result.exports);
const bits64 = value => new BigUint64Array(new Float64Array([value]).buffer)[0].toString();
const bits32 = value => new Uint32Array(new Float32Array([value]).buffer)[0].toString();
const a = await instantiate();
const b = await instantiate();
const edge = [0n, -1n, 9223372036854775807n].map(seed => {
  a.__sjulia_rng_seed(seed);
  return Array.from({length:8}, () => bits64(a.uniform64())).join(',');
});
a.__sjulia_rng_seed(42n);
const first = Array.from({length:16}, () => bits64(a.uniform64())).join(',');
a.__sjulia_rng_seed(42n);
const reseeded = Array.from({length:16}, () => bits64(a.uniform64())).join(',');
a.__sjulia_rng_seed(42n);
b.__sjulia_rng_seed(43n);
const independentA = bits64(a.uniform64());
const independentB = bits64(b.uniform64());
a.__sjulia_rng_seed(42n);
const float32 = Array.from({length:1024}, () => bits32(a.uniform32())).join(',');
console.log(JSON.stringify({edge, first, reseeded, independentA, independentB, float32}));
"#,
    );
    let decoded: serde_json::Value = serde_json::from_str(&actual).expect("decode RNG QA JSON");
    assert_eq!(decoded["edge"], serde_json::json!(expected64));
    assert_eq!(decoded["first"], decoded["reseeded"]);
    assert_ne!(decoded["independentA"], decoded["independentB"]);
    assert_eq!(decoded["float32"], expected32);
}
