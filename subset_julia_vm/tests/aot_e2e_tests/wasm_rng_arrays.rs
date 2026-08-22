use subset_julia_vm_bytecode::rng::{randn, RngLike, Xoshiro};

use super::support::{compile_rng_module, run_node};

#[test]
fn wasm_rng_array_rank_1_uniform() {
    let source = "uniform_vec()::Vector{Float64} = rand(5)";
    let wasm = compile_rng_module(source, &[("uniform_vec", vec![])]);
    let mut oracle = Xoshiro::new(42);
    let expected = (0..5)
        .map(|_| oracle.next_f64().to_bits().to_string())
        .collect::<Vec<_>>()
        .join(",");
    let actual = run_node(
        &wasm,
        r#"
const e = (await WebAssembly.instantiate(module, {})).exports;
e.__sjulia_rng_seed(42n);
const bits = x => new BigUint64Array(new Float64Array([x]).buffer)[0].toString();
const result = e.uniform_vec();
console.log(JSON.stringify({
  imports: WebAssembly.Module.imports(module).length,
  values: Array.from({length:5}, (_, i) => bits(result[i])).join(','),
}));
"#,
    );
    let decoded: serde_json::Value = serde_json::from_str(&actual).expect("decode array QA JSON");
    assert_eq!(decoded["imports"], 0);
    assert_eq!(decoded["values"], expected);
}

#[test]
fn wasm_rng_array_rank_1_normal() {
    let source = "normal_vec()::Vector{Float64} = randn(5)";
    let wasm = compile_rng_module(source, &[("normal_vec", vec![])]);
    let mut oracle = Xoshiro::new(42);
    let expected = (0..5)
        .map(|_| randn(&mut oracle).to_bits().to_string())
        .collect::<Vec<_>>()
        .join(",");
    let actual = run_node(
        &wasm,
        r#"
const e = (await WebAssembly.instantiate(module, {})).exports;
e.__sjulia_rng_seed(42n);
const bits = x => new BigUint64Array(new Float64Array([x]).buffer)[0].toString();
const result = e.normal_vec();
console.log(JSON.stringify({
  imports: WebAssembly.Module.imports(module).length,
  values: Array.from({length:5}, (_, i) => bits(result[i])).join(','),
}));
"#,
    );
    let decoded: serde_json::Value = serde_json::from_str(&actual).expect("decode array QA JSON");
    assert_eq!(decoded["imports"], 0);
    assert_eq!(decoded["values"], expected);
}

#[test]
fn wasm_rng_array_rank_2_uniform() {
    let source = "uniform_mat()::Matrix{Float64} = rand(3, 4)";
    let wasm = compile_rng_module(source, &[("uniform_mat", vec![])]);
    let mut oracle = Xoshiro::new(42);
    // Column-major order: (0,0), (1,0), (2,0), (0,1), (1,1), (2,1), ...
    let expected = (0..12)
        .map(|_| oracle.next_f64().to_bits().to_string())
        .collect::<Vec<_>>()
        .join(",");
    let actual = run_node(
        &wasm,
        r#"
const e = (await WebAssembly.instantiate(module, {})).exports;
e.__sjulia_rng_seed(42n);
const bits = x => new BigUint64Array(new Float64Array([x]).buffer)[0].toString();
const result = e.uniform_mat();
console.log(JSON.stringify({
  imports: WebAssembly.Module.imports(module).length,
  values: Array.from({length:12}, (_, i) => bits(result[i])).join(','),
}));
"#,
    );
    let decoded: serde_json::Value = serde_json::from_str(&actual).expect("decode array QA JSON");
    assert_eq!(decoded["imports"], 0);
    assert_eq!(decoded["values"], expected);
}

#[test]
fn wasm_rng_array_rank_2_normal() {
    let source = "normal_mat()::Matrix{Float64} = randn(3, 4)";
    let wasm = compile_rng_module(source, &[("normal_mat", vec![])]);
    let mut oracle = Xoshiro::new(42);
    // Column-major order: (0,0), (1,0), (2,0), (0,1), (1,1), (2,1), ...
    let expected = (0..12)
        .map(|_| randn(&mut oracle).to_bits().to_string())
        .collect::<Vec<_>>()
        .join(",");
    let actual = run_node(
        &wasm,
        r#"
const e = (await WebAssembly.instantiate(module, {})).exports;
e.__sjulia_rng_seed(42n);
const bits = x => new BigUint64Array(new Float64Array([x]).buffer)[0].toString();
const result = e.normal_mat();
console.log(JSON.stringify({
  imports: WebAssembly.Module.imports(module).length,
  values: Array.from({length:12}, (_, i) => bits(result[i])).join(','),
}));
"#,
    );
    let decoded: serde_json::Value = serde_json::from_str(&actual).expect("decode array QA JSON");
    assert_eq!(decoded["imports"], 0);
    assert_eq!(decoded["values"], expected);
}

#[test]
fn wasm_rng_array_f32_demotion() {
    let source = "uniform32()::Vector{Float32} = rand(8)";
    let wasm = compile_rng_module(source, &[("uniform32", vec![])]);
    let mut oracle = Xoshiro::new(42);
    let expected = (0..8)
        .map(|_| (oracle.next_f64() as f32).to_bits().to_string())
        .collect::<Vec<_>>()
        .join(",");
    let actual = run_node(
        &wasm,
        r#"
const e = (await WebAssembly.instantiate(module, {})).exports;
e.__sjulia_rng_seed(42n);
const bits = x => new Uint32Array(new Float32Array([x]).buffer)[0].toString();
const result = e.uniform32();
console.log(JSON.stringify({
  imports: WebAssembly.Module.imports(module).length,
  values: Array.from({length:8}, (_, i) => bits(result[i])).join(','),
}));
"#,
    );
    let decoded: serde_json::Value = serde_json::from_str(&actual).expect("decode array QA JSON");
    assert_eq!(decoded["imports"], 0);
    assert_eq!(decoded["values"], expected);
}

#[test]
fn wasm_rng_array_scalar_interleave() {
    let source = "scalar()::Float64 = rand()\nvec()::Vector{Float64} = rand(3)";
    let wasm = compile_rng_module(source, &[("scalar", vec![]), ("vec", vec![])]);
    let mut oracle = Xoshiro::new(42);
    // Interleave: scalar, vec[0], vec[1], vec[2], scalar, vec[0], vec[1], vec[2], ...
    let expected_scalars = (0..4)
        .map(|_| {
            let s = oracle.next_f64().to_bits().to_string();
            let _ = (0..3).map(|_| oracle.next_f64()).collect::<Vec<_>>();
            s
        })
        .collect::<Vec<_>>()
        .join(",");
    let mut oracle2 = Xoshiro::new(42);
    let expected_vecs = (0..4)
        .flat_map(|_| {
            let _ = oracle2.next_f64();
            (0..3).map(|_| oracle2.next_f64().to_bits().to_string())
        })
        .collect::<Vec<_>>()
        .join(",");
    let actual = run_node(
        &wasm,
        r#"
const e = (await WebAssembly.instantiate(module, {})).exports;
const bits = x => new BigUint64Array(new Float64Array([x]).buffer)[0].toString();
e.__sjulia_rng_seed(42n);
const scalars = Array.from({length:4}, () => {
  const s = bits(e.scalar());
  e.vec();
  return s;
}).join(',');
e.__sjulia_rng_seed(42n);
const vecs = Array.from({length:4}, () => {
  e.scalar();
  return Array.from({length:3}, (_, i) => bits(e.vec()[i]));
}).flat().join(',');
console.log(JSON.stringify({
  imports: WebAssembly.Module.imports(module).length,
  scalars,
  vecs,
}));
"#,
    );
    let decoded: serde_json::Value = serde_json::from_str(&actual).expect("decode interleave QA JSON");
    assert_eq!(decoded["imports"], 0);
    assert_eq!(decoded["scalars"], expected_scalars);
    assert_eq!(decoded["vecs"], expected_vecs);
}

#[test]
fn wasm_rng_array_reseed_determinism() {
    let source = "vec()::Vector{Float64} = rand(5)";
    let wasm = compile_rng_module(source, &[("vec", vec![])]);
    let mut oracle = Xoshiro::new(42);
    let expected = (0..5)
        .map(|_| oracle.next_f64().to_bits().to_string())
        .collect::<Vec<_>>()
        .join(",");
    let actual = run_node(
        &wasm,
        r#"
const e = (await WebAssembly.instantiate(module, {})).exports;
const bits = x => new BigUint64Array(new Float64Array([x]).buffer)[0].toString();
e.__sjulia_rng_seed(42n);
const first = Array.from({length:5}, (_, i) => bits(e.vec()[i])).join(',');
e.__sjulia_rng_seed(42n);
const reseeded = Array.from({length:5}, (_, i) => bits(e.vec()[i])).join(',');
console.log(JSON.stringify({
  imports: WebAssembly.Module.imports(module).length,
  first,
  reseeded,
}));
"#,
    );
    let decoded: serde_json::Value = serde_json::from_str(&actual).expect("decode reseed QA JSON");
    assert_eq!(decoded["imports"], 0);
    assert_eq!(decoded["first"], decoded["reseeded"]);
}

#[test]
fn wasm_rng_array_independent_instances() {
    let source = "vec()::Vector{Float64} = rand(3)";
    let wasm = compile_rng_module(source, &[("vec", vec![])]);
    let actual = run_node(
        &wasm,
        r#"
const e1 = (await WebAssembly.instantiate(module, {})).exports;
const e2 = (await WebAssembly.instantiate(module, {})).exports;
const bits = x => new BigUint64Array(new Float64Array([x]).buffer)[0].toString();
e1.__sjulia_rng_seed(42n);
e2.__sjulia_rng_seed(43n);
const vec1 = Array.from({length:3}, (_, i) => bits(e1.vec()[i])).join(',');
const vec2 = Array.from({length:3}, (_, i) => bits(e2.vec()[i])).join(',');
console.log(JSON.stringify({
  imports: WebAssembly.Module.imports(module).length,
  vec1,
  vec2,
}));
"#,
    );
    let decoded: serde_json::Value = serde_json::from_str(&actual).expect("decode instances QA JSON");
    assert_eq!(decoded["imports"], 0);
    assert_ne!(decoded["vec1"], decoded["vec2"]);
}
