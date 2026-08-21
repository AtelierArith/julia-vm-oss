use std::fs;
use std::process::{Command, Stdio};
use std::thread;
use std::time::{Duration, Instant};

use subset_julia_vm::aot::codegen::CAbiExport;
use subset_julia_vm::aot::types::StaticType;
use subset_julia_vm::aot::{compile_wasm_source, AotBackend, CompileConfig};
use subset_julia_vm_bytecode::rng::{RngLike, Xoshiro};

fn compile_rng_module(source: &str, exports: &[(&str, Vec<StaticType>)]) -> Vec<u8> {
    let config = CompileConfig {
        backend: AotBackend::Wasm,
        c_abi_exports: exports
            .iter()
            .map(|(name, args)| CAbiExport::with_arg_types(*name, *name, args.clone()))
            .collect(),
        ..CompileConfig::default()
    };
    compile_wasm_source(source, &config)
        .expect("generated-Wasm RNG source should compile")
        .wasm_bytes
}

fn run_node(wasm: &[u8], javascript: &str) -> String {
    let dir = tempfile::tempdir().expect("create RNG QA directory");
    let wasm_path = dir.path().join("rng.wasm");
    let script_path = dir.path().join("rng.mjs");
    fs::write(&wasm_path, wasm).expect("write RNG Wasm");
    fs::write(
        &script_path,
        format!(
            "const bytes = await import('node:fs').then(fs => fs.readFileSync({wasm_path:?}));\nconst module = await WebAssembly.compile(bytes);\n{javascript}",
        ),
    )
    .expect("write RNG Node runner");
    let mut child = Command::new("node")
        .arg(&script_path)
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .expect("run RNG Wasm through Node");
    let deadline = Instant::now() + Duration::from_secs(10);
    let status = loop {
        if let Some(status) = child.try_wait().expect("poll RNG Node runner") {
            break status;
        }
        if Instant::now() >= deadline {
            child.kill().expect("terminate hung RNG runner");
            let _ = child.wait();
            panic!("RNG Node runner exceeded ten-second deadline");
        }
        thread::sleep(Duration::from_millis(10));
    };
    let output = child.wait_with_output().expect("collect RNG Node output");
    assert!(
        status.success(),
        "Node RNG QA failed\nstdout:\n{}\nstderr:\n{}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );
    String::from_utf8(output.stdout)
        .expect("RNG output should be UTF-8")
        .trim()
        .to_string()
}

fn function_type_indices(wasm: &[u8]) -> Vec<u32> {
    let mut cursor = 8;
    while cursor < wasm.len() {
        let section = wasm[cursor];
        cursor += 1;
        let Some(length) = read_leb(wasm, &mut cursor) else {
            panic!("malformed Wasm section length");
        };
        let end = cursor + length as usize;
        if section == 3 {
            let Some(count) = read_leb(wasm, &mut cursor) else {
                panic!("malformed Wasm function section");
            };
            let mut indices = Vec::with_capacity(count as usize);
            for _ in 0..count {
                let Some(index) = read_leb(wasm, &mut cursor) else {
                    panic!("malformed Wasm type index");
                };
                indices.push(index);
            }
            return indices;
        }
        cursor = end;
    }
    panic!("missing Wasm function section");
}

fn read_leb(bytes: &[u8], cursor: &mut usize) -> Option<u32> {
    let mut value = 0;
    let mut shift = 0;
    loop {
        let byte = *bytes.get(*cursor)?;
        *cursor += 1;
        value |= u32::from(byte & 0x7f) << shift;
        if byte & 0x80 == 0 {
            return Some(value);
        }
        shift += 7;
        if shift >= 32 {
            return None;
        }
    }
}

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
    assert_eq!(
        decoded["edge"],
        serde_json::Value::Array(
            expected64
                .into_iter()
                .map(serde_json::Value::String)
                .collect()
        )
    );
    assert_eq!(decoded["first"], decoded["reseeded"]);
    assert_ne!(decoded["independentA"], decoded["independentB"]);
    assert_eq!(decoded["float32"], expected32);
}

#[test]
fn wasm_runtime_helpers_preserve_function_section_signatures() {
    // Given: a generated module with one user function and every runtime helper.
    let wasm = compile_rng_module("uniform()::Float64 = rand()", &[("uniform", vec![])]);

    assert_eq!(
        function_type_indices(&wasm),
        vec![0, 1, 2, 3, 4, 5, 6, 7, 8]
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
