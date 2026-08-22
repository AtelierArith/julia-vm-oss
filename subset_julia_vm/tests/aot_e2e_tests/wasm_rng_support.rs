use std::fs;
use std::process::{Command, Stdio};
use std::thread;
use std::time::{Duration, Instant};

use subset_julia_vm::aot::codegen::CAbiExport;
use subset_julia_vm::aot::types::StaticType;
use subset_julia_vm::aot::{compile_wasm_source, AotBackend, CompileConfig};

pub(super) fn compile_rng_module(source: &str, exports: &[(&str, Vec<StaticType>)]) -> Vec<u8> {
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

pub(super) fn run_node(wasm: &[u8], javascript: &str) -> String {
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

pub(super) fn function_type_indices(wasm: &[u8]) -> Vec<u32> {
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
