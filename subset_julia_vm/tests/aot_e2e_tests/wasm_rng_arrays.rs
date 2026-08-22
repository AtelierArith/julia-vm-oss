use subset_julia_vm_bytecode::rng::{randn, RngLike, Xoshiro};

use super::support::{compile_rng_module, run_node};

const ARRAY_DECODER: &str = r#"
const array = (exports, descriptor, elementSize, rank) => {
  const base = Number(descriptor);
  const view = new DataView(exports.memory.buffer);
  if (view.getUint32(base, true) !== 2) throw new Error('expected ABI v2 array descriptor');
  if (view.getUint32(base + 20, true) !== rank) throw new Error('unexpected array rank');
  const dataPtr = view.getUint32(base + 24, true);
  const count = Number(view.getBigUint64(base + 32, true));
  return elementSize === 8
    ? new Float64Array(exports.memory.buffer, dataPtr, count)
    : new Float32Array(exports.memory.buffer, dataPtr, count);
};
"#;

fn run_node_array(wasm: &[u8], javascript: &str) -> String {
    run_node(wasm, &format!("{ARRAY_DECODER}\n{javascript}"))
}

#[test]
fn wasm_rng_array_rank_1_uniform() {
    let source = "uniform_vec()::Vector{Float64} = rand(5)";
    let wasm = compile_rng_module(source, &[("uniform_vec", vec![])]);
    let mut oracle = Xoshiro::new(42);
    let expected = (0..5)
        .map(|_| oracle.next_f64().to_bits().to_string())
        .collect::<Vec<_>>()
        .join(",");
    let actual = run_node_array(
        &wasm,
        r#"
const e = (await WebAssembly.instantiate(module, {})).exports;
e.__sjulia_rng_seed(42n);
const bits = x => new BigUint64Array(new Float64Array([x]).buffer)[0].toString();
const result = array(e, e.uniform_vec(), 8, 1);
console.log(JSON.stringify({
  imports: WebAssembly.Module.imports(module).length,
  values: Array.from(result, bits).join(','),
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
    let actual = run_node_array(
        &wasm,
        r#"
const e = (await WebAssembly.instantiate(module, {})).exports;
e.__sjulia_rng_seed(42n);
const bits = x => new BigUint64Array(new Float64Array([x]).buffer)[0].toString();
const result = array(e, e.normal_vec(), 8, 1);
console.log(JSON.stringify({
  imports: WebAssembly.Module.imports(module).length,
  values: Array.from(result, bits).join(','),
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
    let actual = run_node_array(
        &wasm,
        r#"
const e = (await WebAssembly.instantiate(module, {})).exports;
e.__sjulia_rng_seed(42n);
const bits = x => new BigUint64Array(new Float64Array([x]).buffer)[0].toString();
const result = array(e, e.uniform_mat(), 8, 2);
console.log(JSON.stringify({
  imports: WebAssembly.Module.imports(module).length,
  values: Array.from(result, bits).join(','),
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
    let actual = run_node_array(
        &wasm,
        r#"
const e = (await WebAssembly.instantiate(module, {})).exports;
e.__sjulia_rng_seed(42n);
const bits = x => new BigUint64Array(new Float64Array([x]).buffer)[0].toString();
const result = array(e, e.normal_mat(), 8, 2);
console.log(JSON.stringify({
  imports: WebAssembly.Module.imports(module).length,
  values: Array.from(result, bits).join(','),
}));
"#,
    );
    let decoded: serde_json::Value = serde_json::from_str(&actual).expect("decode array QA JSON");
    assert_eq!(decoded["imports"], 0);
    assert_eq!(decoded["values"], expected);
}

#[test]
fn wasm_rng_array_f32_demotion() {
    let source = "uniform32()::Vector{Float32} = rand(Float32, 8)";
    let wasm = compile_rng_module(source, &[("uniform32", vec![])]);
    let mut oracle = Xoshiro::new(42);
    let expected = (0..8)
        .map(|_| (oracle.next_f64() as f32).to_bits().to_string())
        .collect::<Vec<_>>()
        .join(",");
    let actual = run_node_array(
        &wasm,
        r#"
const e = (await WebAssembly.instantiate(module, {})).exports;
e.__sjulia_rng_seed(42n);
const bits = x => new Uint32Array(new Float32Array([x]).buffer)[0].toString();
const result = array(e, e.uniform32(), 4, 1);
console.log(JSON.stringify({
  imports: WebAssembly.Module.imports(module).length,
  values: Array.from(result, bits).join(','),
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
    let expected_vecs = {
        let mut results = Vec::new();
        for _ in 0..4 {
            let _ = oracle2.next_f64();
            for _ in 0..3 {
                results.push(oracle2.next_f64().to_bits().to_string());
            }
        }
        results.join(",")
    };
    let actual = run_node_array(
        &wasm,
        r#"
const e = (await WebAssembly.instantiate(module, {})).exports;
const bits = x => new BigUint64Array(new Float64Array([x]).buffer)[0].toString();
e.__sjulia_rng_seed(42n);
const scalars = Array.from({length:4}, () => {
  const s = bits(e.scalar());
  array(e, e.vec(), 8, 1);
  return s;
}).join(',');
e.__sjulia_rng_seed(42n);
const vecs = Array.from({length:4}, () => {
  e.scalar();
  return Array.from(array(e, e.vec(), 8, 1), bits);
}).flat().join(',');
console.log(JSON.stringify({
  imports: WebAssembly.Module.imports(module).length,
  scalars,
  vecs,
}));
"#,
    );
    let decoded: serde_json::Value =
        serde_json::from_str(&actual).expect("decode interleave QA JSON");
    assert_eq!(decoded["imports"], 0);
    assert_eq!(decoded["scalars"], expected_scalars);
    assert_eq!(decoded["vecs"], expected_vecs);
}

#[test]
fn wasm_rng_array_reseed_determinism() {
    let source = "vec()::Vector{Float64} = rand(5)";
    let wasm = compile_rng_module(source, &[("vec", vec![])]);
    let actual = run_node_array(
        &wasm,
        r#"
const e = (await WebAssembly.instantiate(module, {})).exports;
const bits = x => new BigUint64Array(new Float64Array([x]).buffer)[0].toString();
e.__sjulia_rng_seed(42n);
const first = Array.from(array(e, e.vec(), 8, 1), bits).join(',');
e.__sjulia_rng_seed(42n);
const reseeded = Array.from(array(e, e.vec(), 8, 1), bits).join(',');
console.log(JSON.stringify({
  imports: WebAssembly.Module.imports(module).length,
  first,
  reseeded,
}));
"#,
    );
    let decoded: serde_json::Value = serde_json::from_str(&actual).expect("decode reseed QA JSON");
    assert_eq!(decoded["imports"], 0);
    let mut oracle = Xoshiro::new(42);
    let expected = (0..5)
        .map(|_| oracle.next_f64().to_bits().to_string())
        .collect::<Vec<_>>()
        .join(",");
    assert_eq!(decoded["first"], expected);
    assert_eq!(decoded["reseeded"], expected);
}

#[test]
fn wasm_rng_array_independent_instances() {
    let source = "vec()::Vector{Float64} = rand(3)";
    let wasm = compile_rng_module(source, &[("vec", vec![])]);
    let actual = run_node_array(
        &wasm,
        r#"
const e1 = (await WebAssembly.instantiate(module, {})).exports;
const e2 = (await WebAssembly.instantiate(module, {})).exports;
const bits = x => new BigUint64Array(new Float64Array([x]).buffer)[0].toString();
e1.__sjulia_rng_seed(42n);
e2.__sjulia_rng_seed(43n);
const vec1 = Array.from(array(e1, e1.vec(), 8, 1), bits).join(',');
const vec2 = Array.from(array(e2, e2.vec(), 8, 1), bits).join(',');
console.log(JSON.stringify({
  imports: WebAssembly.Module.imports(module).length,
  vec1,
  vec2,
}));
"#,
    );
    let decoded: serde_json::Value =
        serde_json::from_str(&actual).expect("decode instances QA JSON");
    assert_eq!(decoded["imports"], 0);
    assert_ne!(decoded["vec1"], decoded["vec2"]);
}

#[test]
fn wasm_rng_array_rank_3_uniform() {
    let source = "uniform_3d()::Array{Float64,3} = rand(2, 3, 4)";
    let wasm = compile_rng_module(source, &[("uniform_3d", vec![])]);
    let mut oracle = Xoshiro::new(42);
    // Column-major order: 2*3*4 = 24 elements
    let expected = (0..24)
        .map(|_| oracle.next_f64().to_bits().to_string())
        .collect::<Vec<_>>()
        .join(",");
    let actual = run_node_array(
        &wasm,
        r#"
const e = (await WebAssembly.instantiate(module, {})).exports;
e.__sjulia_rng_seed(42n);
const bits = x => new BigUint64Array(new Float64Array([x]).buffer)[0].toString();
const result = array(e, e.uniform_3d(), 8, 3);
console.log(JSON.stringify({
  imports: WebAssembly.Module.imports(module).length,
  values: Array.from(result, bits).join(','),
}));
"#,
    );
    let decoded: serde_json::Value = serde_json::from_str(&actual).expect("decode rank 3 QA JSON");
    assert_eq!(decoded["imports"], 0);
    assert_eq!(decoded["values"], expected);
}

#[test]
fn wasm_rng_array_rank_5_uniform() {
    let source = "uniform_5d()::Array{Float64,5} = rand(2, 2, 2, 2, 2)";
    let wasm = compile_rng_module(source, &[("uniform_5d", vec![])]);
    let mut oracle = Xoshiro::new(42);
    // Column-major order: 2^5 = 32 elements
    let expected = (0..32)
        .map(|_| oracle.next_f64().to_bits().to_string())
        .collect::<Vec<_>>()
        .join(",");
    let actual = run_node_array(
        &wasm,
        r#"
const e = (await WebAssembly.instantiate(module, {})).exports;
e.__sjulia_rng_seed(42n);
const bits = x => new BigUint64Array(new Float64Array([x]).buffer)[0].toString();
const result = array(e, e.uniform_5d(), 8, 5);
console.log(JSON.stringify({
  imports: WebAssembly.Module.imports(module).length,
  values: Array.from(result, bits).join(','),
}));
"#,
    );
    let decoded: serde_json::Value = serde_json::from_str(&actual).expect("decode rank 5 QA JSON");
    assert_eq!(decoded["imports"], 0);
    assert_eq!(decoded["values"], expected);
}

#[test]
fn wasm_rng_array_rank_8_uniform() {
    let source = "uniform_8d()::Array{Float64,8} = rand(2, 2, 2, 2, 2, 2, 2, 2)";
    let wasm = compile_rng_module(source, &[("uniform_8d", vec![])]);
    let mut oracle = Xoshiro::new(42);
    // Column-major order: 2^8 = 256 elements
    let expected = (0..256)
        .map(|_| oracle.next_f64().to_bits().to_string())
        .collect::<Vec<_>>()
        .join(",");
    let actual = run_node_array(
        &wasm,
        r#"
const e = (await WebAssembly.instantiate(module, {})).exports;
e.__sjulia_rng_seed(42n);
const bits = x => new BigUint64Array(new Float64Array([x]).buffer)[0].toString();
const result = array(e, e.uniform_8d(), 8, 8);
console.log(JSON.stringify({
  imports: WebAssembly.Module.imports(module).length,
  values: Array.from(result, bits).join(','),
}));
"#,
    );
    let decoded: serde_json::Value = serde_json::from_str(&actual).expect("decode rank 8 QA JSON");
    assert_eq!(decoded["imports"], 0);
    assert_eq!(decoded["values"], expected);
}

#[test]
fn wasm_rng_array_rank_3_normal() {
    let source = "normal_3d()::Array{Float64,3} = randn(2, 3, 4)";
    let wasm = compile_rng_module(source, &[("normal_3d", vec![])]);
    let mut oracle = Xoshiro::new(42);
    // Column-major order: 2*3*4 = 24 elements
    let expected = (0..24)
        .map(|_| randn(&mut oracle).to_bits().to_string())
        .collect::<Vec<_>>()
        .join(",");
    let actual = run_node_array(
        &wasm,
        r#"
const e = (await WebAssembly.instantiate(module, {})).exports;
e.__sjulia_rng_seed(42n);
const bits = x => new BigUint64Array(new Float64Array([x]).buffer)[0].toString();
const result = array(e, e.normal_3d(), 8, 3);
console.log(JSON.stringify({
  imports: WebAssembly.Module.imports(module).length,
  values: Array.from(result, bits).join(','),
}));
"#,
    );
    let decoded: serde_json::Value = serde_json::from_str(&actual).expect("decode rank 3 QA JSON");
    assert_eq!(decoded["imports"], 0);
    assert_eq!(decoded["values"], expected);
}

#[test]
fn wasm_rng_array_zero_dimensions_f64() {
    let source = "empty()::Vector{Float64} = rand(0)";
    let wasm = compile_rng_module(source, &[("empty", vec![])]);
    let actual = run_node_array(
        &wasm,
        r#"
const e = (await WebAssembly.instantiate(module, {})).exports;
e.__sjulia_rng_seed(42n);
const result = array(e, e.empty(), 8, 1);
console.log(JSON.stringify({
  imports: WebAssembly.Module.imports(module).length,
  length: result.length,
}));
"#,
    );
    let decoded: serde_json::Value = serde_json::from_str(&actual).expect("decode empty QA JSON");
    assert_eq!(decoded["imports"], 0);
    assert_eq!(decoded["length"], 0);
}

#[test]
fn wasm_rng_array_zero_dimensions_f32() {
    let source = "empty32()::Vector{Float32} = rand(Float32, 0)";
    let wasm = compile_rng_module(source, &[("empty32", vec![])]);
    let actual = run_node_array(
        &wasm,
        r#"
const e = (await WebAssembly.instantiate(module, {})).exports;
e.__sjulia_rng_seed(42n);
const result = array(e, e.empty32(), 4, 1);
console.log(JSON.stringify({
  imports: WebAssembly.Module.imports(module).length,
  length: result.length,
}));
"#,
    );
    let decoded: serde_json::Value = serde_json::from_str(&actual).expect("decode empty32 QA JSON");
    assert_eq!(decoded["imports"], 0);
    assert_eq!(decoded["length"], 0);
}

#[test]
fn wasm_rng_array_first_1024_elements_oracle() {
    let source = "large()::Vector{Float64} = rand(1024)";
    let wasm = compile_rng_module(source, &[("large", vec![])]);
    let mut oracle = Xoshiro::new(42);
    let expected = (0..1024)
        .map(|_| oracle.next_f64().to_bits().to_string())
        .collect::<Vec<_>>()
        .join(",");
    let actual = run_node_array(
        &wasm,
        r#"
const e = (await WebAssembly.instantiate(module, {})).exports;
e.__sjulia_rng_seed(42n);
const bits = x => new BigUint64Array(new Float64Array([x]).buffer)[0].toString();
const result = array(e, e.large(), 8, 1);
console.log(JSON.stringify({
  imports: WebAssembly.Module.imports(module).length,
  values: Array.from(result, bits).join(','),
}));
"#,
    );
    let decoded: serde_json::Value = serde_json::from_str(&actual).expect("decode 1024 QA JSON");
    assert_eq!(decoded["imports"], 0);
    assert_eq!(decoded["values"], expected);
}

#[test]
fn wasm_rng_array_descriptor_abi_v2() {
    let source = "vec()::Vector{Float64} = rand(5)";
    let wasm = compile_rng_module(source, &[("vec", vec![])]);
    let actual = run_node_array(
        &wasm,
        r#"
const e = (await WebAssembly.instantiate(module, {})).exports;
e.__sjulia_rng_seed(42n);
const descriptor = e.vec();
const view = new DataView(e.memory.buffer);
const abi_version = view.getUint32(Number(descriptor), true);
const rank = view.getUint32(Number(descriptor) + 20, true);
const count = Number(view.getBigUint64(Number(descriptor) + 32, true));
console.log(JSON.stringify({
  imports: WebAssembly.Module.imports(module).length,
  abi_version,
  rank,
  count,
}));
"#,
    );
    let decoded: serde_json::Value =
        serde_json::from_str(&actual).expect("decode descriptor QA JSON");
    assert_eq!(decoded["imports"], 0);
    assert_eq!(decoded["abi_version"], 2);
    assert_eq!(decoded["rank"], 1);
    assert_eq!(decoded["count"], 5);
}
