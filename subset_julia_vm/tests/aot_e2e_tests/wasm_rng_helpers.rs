use super::support::{compile_rng_module, function_type_indices, run_node};

#[test]
fn wasm_runtime_helpers_preserve_function_section_signatures() {
    let wasm = compile_rng_module("uniform()::Float64 = rand()", &[("uniform", vec![])]);
    assert_eq!(
        function_type_indices(&wasm),
        vec![0, 1, 2, 3, 4, 5, 6, 7, 8, 9]
    );
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
  alloc: typeof e.__sjulia_alloc, free: typeof e.__sjulia_free,
  rngSeed: typeof e.__sjulia_rng_seed, drop: typeof e.__sjulia_drop,
  layoutTable: typeof e.__sjulia_layout_table, layoutCount: typeof e.__sjulia_layout_count,
  abi: typeof e.__sjulia_wasm_abi_version, rngNextExported: Object.hasOwn(e, '__sjulia_rng_next'),
  allocResult: typeof e.__sjulia_alloc(0n, 8), freeResult: e.__sjulia_free(freed) === undefined,
  seedResult: e.__sjulia_rng_seed(42n) === undefined, dropResult: e.__sjulia_drop(descriptor) === undefined,
  layoutTableResult: typeof e.__sjulia_layout_table(), layoutCountResult: typeof e.__sjulia_layout_count(),
  abiResult: typeof e.__sjulia_wasm_abi_version(),
};
console.log(JSON.stringify(results));
"#,
    );
    let decoded: serde_json::Value = serde_json::from_str(&actual).expect("decode helper QA JSON");
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
