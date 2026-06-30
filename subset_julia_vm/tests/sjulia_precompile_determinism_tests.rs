//! Determinism test for `sjulia --precompile-base`.
//!
//! `target/base_cache.bin` is embedded into the WASM build via `include_bytes!`
//! (subset_julia_vm/build.rs declares it as a build dependency). Any byte
//! difference forces cargo to rebuild subset_julia_vm + subset_julia_vm_web,
//! which combined with `lto = true` makes the WASM relink the dominant cost of
//! `scripts/wasm_build_with_cache.sh`.
//!
//! Two independent processes must produce byte-identical cache files for the
//! same prelude — this regression-tests the deterministic-serialization fix
//! in `subset_julia_vm/src/compile/precompile.rs`.

use std::fs;
use std::path::{Path, PathBuf};
use std::process::Command;

fn sjulia_bin() -> &'static str {
    env!("CARGO_BIN_EXE_sjulia")
}

fn unique_tmp(stem: &str) -> PathBuf {
    let mut path = std::env::temp_dir();
    path.push(format!(
        "sjulia_precompile_{}_{}_{}.bin",
        stem,
        std::process::id(),
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map(|d| d.as_nanos())
            .unwrap_or(0),
    ));
    path
}

fn precompile_base_to(path: &Path) {
    let out = Command::new(sjulia_bin())
        .args(["--precompile-base", path.to_str().expect("utf-8 tmp path")])
        .output()
        .expect("failed to spawn sjulia --precompile-base");
    assert!(
        out.status.success(),
        "sjulia --precompile-base failed (status={:?})\nstdout:\n{}\nstderr:\n{}",
        out.status,
        String::from_utf8_lossy(&out.stdout),
        String::from_utf8_lossy(&out.stderr),
    );
}

fn precompile_prelude_to(path: &Path) {
    let out = Command::new(sjulia_bin())
        .args([
            "--precompile-prelude",
            path.to_str().expect("utf-8 tmp path"),
        ])
        .output()
        .expect("failed to spawn sjulia --precompile-prelude");
    assert!(
        out.status.success(),
        "sjulia --precompile-prelude failed (status={:?})\nstdout:\n{}\nstderr:\n{}",
        out.status,
        String::from_utf8_lossy(&out.stdout),
        String::from_utf8_lossy(&out.stderr),
    );
}

#[test]
fn precompile_base_is_deterministic_across_processes() {
    let path_a = unique_tmp("a");
    let path_b = unique_tmp("b");

    precompile_base_to(&path_a);
    precompile_base_to(&path_b);

    let bytes_a = fs::read(&path_a).expect("read cache A");
    let bytes_b = fs::read(&path_b).expect("read cache B");

    let _ = fs::remove_file(&path_a);
    let _ = fs::remove_file(&path_b);

    assert_eq!(
        bytes_a.len(),
        bytes_b.len(),
        "cache size must match across runs"
    );
    assert!(
        bytes_a == bytes_b,
        "sjulia --precompile-base produced different bytes for the same prelude across two \
         independent processes — serialization is non-deterministic. This breaks Cargo's \
         incremental tracking of target/base_cache.bin (embedded into WASM via include_bytes!) \
         and forces a full subset_julia_vm_web relink every time the cache is regenerated."
    );
}

#[test]
fn precompile_prelude_is_deterministic_across_processes() {
    let path_a = unique_tmp("prelude_a");
    let path_b = unique_tmp("prelude_b");

    precompile_prelude_to(&path_a);
    precompile_prelude_to(&path_b);

    let bytes_a = fs::read(&path_a).expect("read prelude cache A");
    let bytes_b = fs::read(&path_b).expect("read prelude cache B");

    let _ = fs::remove_file(&path_a);
    let _ = fs::remove_file(&path_b);

    assert_eq!(
        bytes_a.len(),
        bytes_b.len(),
        "prelude cache size must match across runs"
    );
    assert!(
        bytes_a == bytes_b,
        "sjulia --precompile-prelude produced different bytes for the same prelude across two \
         independent processes. This breaks Cargo's incremental tracking of \
         target/prelude_program_cache.bin when it is embedded into WASM via include_bytes!."
    );
}
