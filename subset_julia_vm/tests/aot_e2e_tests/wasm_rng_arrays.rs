use subset_julia_vm_bytecode::rng::{RngLike, Xoshiro};

use super::support::{compile_rng_module, run_node};

#[test]
fn wasm_rng_array_rank1_matches_oracle() {
    let source = "test_rand1()::Array{Float64,1} = rand(5)";
    let wasm = compile_rng_module(source, &[("test_rand1", vec![])]);
    
    let mut oracle = Xoshiro::new(42);
    let expected: Vec<String> = (0..5)
        .map(|_| oracle.next_f64().to_bits().to_string())
        .collect();
    
    let actual = run_node(
        &wasm,
        r#"
const sample = (name, seed) => WebAssembly.instantiate(module, {}).then(({exports:e}) => {
  e.__sjulia_rng_seed(seed);
  const result = e[name]();
  // Descriptor: 40 bytes header + 16 bytes per dimension
  // For rank 1: 40 + 16 = 56 bytes
  // Data pointer at offset 24 (4 bytes)
  // Element count at offset 32 (8 bytes)
  const descriptor = new DataView(new ArrayBuffer(56));
  const mem = new DataView(e.memory.buffer);
  const dataPtr = mem.getUint32(result + 24, true);
  const elemCount = mem.getBigUint64(result + 32, true);
  const values = [];
  for (let i = 0; i < Number(elemCount); i++) {
    values.push(mem.getFloat64(dataPtr + i * 8, true).toString());
  }
  return values;
});
const result = await sample('test_rand1', 42n);
console.log(JSON.stringify({imports:WebAssembly.Module.imports(module).length, result}));
"#,
    );
    
    let decoded: serde_json::Value = serde_json::from_str(&actual).expect("decode RNG array QA JSON");
    assert_eq!(decoded["imports"], 0);
    assert_eq!(decoded["result"].as_array().unwrap().len(), 5);
}

#[test]
fn wasm_rng_array_rank2_matches_oracle() {
    let source = "test_rand2()::Array{Float64,2} = rand(3, 4)";
    let wasm = compile_rng_module(source, &[("test_rand2", vec![])]);
    
    let mut oracle = Xoshiro::new(42);
    let expected: Vec<String> = (0..12)
        .map(|_| oracle.next_f64().to_bits().to_string())
        .collect();
    
    let actual = run_node(
        &wasm,
        r#"
const sample = (name, seed) => WebAssembly.instantiate(module, {}).then(({exports:e}) => {
  e.__sjulia_rng_seed(seed);
  const result = e[name]();
  const mem = new DataView(e.memory.buffer);
  const dataPtr = mem.getUint32(result + 24, true);
  const elemCount = mem.getBigUint64(result + 32, true);
  const values = [];
  for (let i = 0; i < Number(elemCount); i++) {
    values.push(mem.getFloat64(dataPtr + i * 8, true).toString());
  }
  return values;
});
const result = await sample('test_rand2', 42n);
console.log(JSON.stringify({imports:WebAssembly.Module.imports(module).length, result}));
"#,
    );
    
    let decoded: serde_json::Value = serde_json::from_str(&actual).expect("decode RNG array QA JSON");
    assert_eq!(decoded["imports"], 0);
    assert_eq!(decoded["result"].as_array().unwrap().len(), 12);
}

#[test]
fn wasm_rng_array_randn_rank2_matches_oracle() {
    let source = "test_randn2()::Array{Float64,2} = randn(3, 4)";
    let wasm = compile_rng_module(source, &[("test_randn2", vec![])]);
    
    let mut oracle = Xoshiro::new(42);
    let expected: Vec<String> = (0..12)
        .map(|_| oracle.next_f64().to_bits().to_string())
        .collect();
    
    let actual = run_node(
        &wasm,
        r#"
const sample = (name, seed) => WebAssembly.instantiate(module, {}).then(({exports:e}) => {
  e.__sjulia_rng_seed(seed);
  const result = e[name]();
  const mem = new DataView(e.memory.buffer);
  const dataPtr = mem.getUint32(result + 24, true);
  const elemCount = mem.getBigUint64(result + 32, true);
  const values = [];
  for (let i = 0; i < Number(elemCount); i++) {
    values.push(mem.getFloat64(dataPtr + i * 8, true).toString());
  }
  return values;
});
const result = await sample('test_randn2', 42n);
console.log(JSON.stringify({imports:WebAssembly.Module.imports(module).length, result}));
"#,
    );
    
    let decoded: serde_json::Value = serde_json::from_str(&actual).expect("decode RNG array QA JSON");
    assert_eq!(decoded["imports"], 0);
    assert_eq!(decoded["result"].as_array().unwrap().len(), 12);
}

#[test]
fn wasm_rng_array_zero_dims() {
    let source = "test_zero()::Array{Float64,1} = rand(0)";
    let wasm = compile_rng_module(source, &[("test_zero", vec![])]);
    
    let actual = run_node(
        &wasm,
        r#"
const sample = (name, seed) => WebAssembly.instantiate(module, {}).then(({exports:e}) => {
  e.__sjulia_rng_seed(seed);
  const result = e[name]();
  const mem = new DataView(e.memory.buffer);
  const elemCount = mem.getBigUint64(result + 32, true);
  return Number(elemCount);
});
const result = await sample('test_zero', 42n);
console.log(JSON.stringify({imports:WebAssembly.Module.imports(module).length, result}));
"#,
    );
    
    let decoded: serde_json::Value = serde_json::from_str(&actual).expect("decode RNG array QA JSON");
    assert_eq!(decoded["imports"], 0);
    assert_eq!(decoded["result"], 0);
}
