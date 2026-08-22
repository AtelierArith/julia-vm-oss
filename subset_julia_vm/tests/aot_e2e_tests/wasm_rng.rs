use subset_julia_vm_bytecode::rng::{randn, RngLike, Xoshiro};

#[path = "wasm_rng_support.rs"]
mod support;
use support::{compile_rng_module, function_type_indices, run_node};

#[test]
fn wasm_rng_matches_xoshiro_uniform_streams() {
    // Given: the repository's pinned Xoshiro seed contract and 1,024-sample oracle.
    let source = "uniform()::Float64 = rand()";
    let wasm = compile_rng_module(source, &[("uniform", vec![])]);
    let mut uniform_oracle = Xoshiro::new(42);
    let expected_uniform = (0..1024)
        .map(|_| uniform_oracle.next_f64().to_bits().to_string())
        .collect::<Vec<_>>()
        .join(",");
    // When: two independent instances seed and draw scalar uniform streams.
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

    // Then: modules have no imports, instance state is independent, and every bit matches.
    assert_eq!(decoded["imports"], 0);
    assert_eq!(decoded["uniformA"], expected_uniform);
    assert_eq!(decoded["uniformB"], expected_uniform);
}

#[test]
fn wasm_rng_reseeds_edge_seeds_and_rounds_float32() {
    // Given: scalar Float64 and Float32 draws and signed i64 seed boundaries.
    let source = r#"
uniform64()::Float64 = rand()
uniform32()::Float32 = rand()
"#;
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

    // When: one instance is repeatedly reseeded and two instances advance independently.
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

    // Then: seed bit patterns are deterministic, reseeding restarts, and f32 rounds once.
    assert_eq!(decoded["edge"], serde_json::json!(expected64));
    assert_eq!(decoded["first"], decoded["reseeded"]);
    assert_ne!(decoded["independentA"], decoded["independentB"]);
    assert_eq!(decoded["float32"], expected32);
}

#[test]
fn wasm_randn_matches_repository_xoshiro_streams() {
    // Given: scalar Float64/Float32 normals and the repository Ziggurat oracle.
    let source = r#"
normal64()::Float64 = randn()
normal32()::Float32 = randn()
"#;
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

    // When: independent Wasm instances sample both scalar result types.
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

    // Then: the import-free sampler matches every oracle bit, including a tail draw.
    assert_eq!(decoded["imports"], 0);
    assert_eq!(decoded["normal64"], expected64.join(","));
    assert_eq!(decoded["normal32"], expected32);
}

#[test]
fn wasm_randn_preserves_seed_and_interleaved_stream_order() {
    // Given: rand and randn sharing one module-local Xoshiro stream.
    let source = r#"
uniform()::Float64 = rand()
normal()::Float64 = randn()
"#;
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

    // When: instances reseed, use edge seeds, and interleave uniform/normal draws.
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

    // Then: reseeding, per-instance state, edge seeds, and consumption order are exact.
    assert_eq!(decoded["edge"], serde_json::json!(expected_edges));
    assert_eq!(decoded["first"], decoded["reseeded"]);
    assert_ne!(decoded["independentA"], decoded["independentB"]);
    assert_eq!(decoded["interleaved"], expected_interleaved);
}

#[test]
fn wasm_runtime_helpers_preserve_function_section_signatures() {
    // Given: a generated module with one user function and every runtime helper.
    let wasm = compile_rng_module("uniform()::Float64 = rand()", &[("uniform", vec![])]);

    assert_eq!(
        function_type_indices(&wasm),
        vec![0, 1, 2, 3, 4, 5, 6, 7, 8, 9]
    );

    // When: Node instantiates the module and calls each exported runtime helper.
    let actual = run_node(
        &wasm,
        r#"
const e = (await WebAssembly.instantiate(module, {})).exports;
const descriptor = e.__sjulia_alloc(56n, 8);
const freed = e.__sjulia_alloc(16n, 8);
const view = new DataView(e.memory.buffer);
view.setUint32(descriptor, 2, true);
view.setUint32(descriptor + 4, 1, true);
view.setUint32(descriptor + 8, 1, true);
view.setUint32(descriptor + 12, 1, true);
view.setUint32(descriptor + 16, 0, true);
view.setUint32(descriptor + 20, 1, true);
view.setUint32(descriptor + 24, 0, true);
view.setUint32(descriptor + 28, 0, true);
view.setBigUint64(descriptor + 32, 0n, true);
view.setBigUint64(descriptor + 40, 0n, true);
view.setBigInt64(descriptor + 48, 1n, true);
const results = {
  alloc: typeof e.__sjulia_alloc,
  free: typeof e.__sjulia_free,
  rngSeed: typeof e.__sjulia_rng_seed,
  drop: typeof e.__sjulia_drop,
  layoutTable: typeof e.__sjulia_layout_table,
  layoutCount: typeof e.__sjulia_layout_count,
  abi: typeof e.__sjulia_wasm_abi_version,
  rngNextExported: Object.hasOwn(e, '__sjulia_rng_next'),
  allocResult: typeof e.__sjulia_alloc(0n, 8),
  freeResult: e.__sjulia_free(freed) === undefined,
  seedResult: e.__sjulia_rng_seed(42n) === undefined,
  dropResult: e.__sjulia_drop(descriptor) === undefined,
  layoutTableResult: typeof e.__sjulia_layout_table(),
  layoutCountResult: typeof e.__sjulia_layout_count(),
  abiResult: typeof e.__sjulia_wasm_abi_version(),
};
console.log(JSON.stringify(results));
"#,
    );
    let decoded: serde_json::Value = serde_json::from_str(&actual).expect("decode helper QA JSON");

    // Then: helper exports have their declared ABI results and rng_next stays internal.
    assert_eq!(decoded["alloc"], "function");
    assert_eq!(decoded["free"], "function");
    assert_eq!(decoded["rngSeed"], "function");
    assert_eq!(decoded["drop"], "function");
    assert_eq!(decoded["layoutTable"], "function");
    assert_eq!(decoded["layoutCount"], "function");
    assert_eq!(decoded["abi"], "function");
    assert_eq!(decoded["rngNextExported"], false);
    assert_eq!(decoded["allocResult"], "number");
    assert_eq!(decoded["freeResult"], true);
    assert_eq!(decoded["seedResult"], true);
    assert_eq!(decoded["dropResult"], true);
    assert_eq!(decoded["layoutTableResult"], "number");
    assert_eq!(decoded["layoutCountResult"], "number");
    assert_eq!(decoded["abiResult"], "number");
}
