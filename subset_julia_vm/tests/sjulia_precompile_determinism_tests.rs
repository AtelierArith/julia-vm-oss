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
//! in `subset_julia_vm_compile/src/compile/precompile.rs`.
//!
//! ## Strength of this guard (Issue #10051 slice B audit)
//!
//! - **Full-payload comparison, not a hash/prefix.** `bytes_a == bytes_b`
//!   compares every byte of the cache (a strict superset of hashing the full
//!   payload — a hash comparison could theoretically collide, a byte
//!   comparison cannot), and the preceding `assert_eq!` on `.len()` localizes
//!   a size mismatch before the full-buffer diff.
//! - **Different hash seeds "for free".** Rust's `std::collections::HashMap`
//!   default `RandomState` draws its per-process SipHash keys from OS entropy
//!   at first use; there is no stable `RUST_*` environment variable that
//!   pins or perturbs it (the nightly-only `-Z` randomize-layout knobs are a
//!   different mechanism and do not affect `HashMap` iteration order). Each
//!   `Command::new(sjulia_bin())` spawn below is a genuinely separate OS
//!   process with its own address space and its own random seed, so this
//!   test already compares across independent hash seeds on every run — a
//!   more realistic seed diversity than any artificial env-var override could
//!   provide from a single process. This is why the two processes are run
//!   back-to-back rather than reusing one process's compiled cache twice.
//! - **Audited at the source (Issue #10051 slice B):** every `HashMap`/
//!   `HashSet` reachable from `SerializedBaseCache` at serialize time is
//!   either emitted in a stable sorted order (`method_tables` by typed key,
//!   `closure_captures` by outer+inner key, `promotion_rules` and
//!   `runtime_specialization_map` pre-sorted before their section is
//!   written) or excluded from the wire format entirely via `#[serde(skip)]`
//!   (`MethodTable::dispatch_cache`/`first_arg_index`/`projection`,
//!   `CompiledProgram::compile_context`/`main_scope_names`) or never
//!   serialized as part of the Base cache at all (`macro_bindings`, rebuilt
//!   fresh on every compile). See `docs/vm/CACHE_ARCHITECTURE.md` for the
//!   full inventory and `precompile.rs`'s
//!   `closure_captures_serialize_deterministically_regardless_of_insertion_order_issue_10051`
//!   / `method_tables_serialize_deterministically_with_typed_key_issue_9197_s7`
//!   for the cheap in-process guards that pin each sorted section directly.

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
        // Force each child to parse and compile its own prelude/Base. Without
        // these opt-outs both children can merely deserialize the same local
        // artifacts, making a byte-equality test blind to compiler-side
        // HashSet iteration such as CreateClosure capture order (Issue #11264).
        .env("SUBSET_JULIA_VM_DISABLE_PERSISTENT_PRELUDE_CACHE", "1")
        .env("SUBSET_JULIA_VM_DISABLE_PERSISTENT_BASE_CACHE", "1")
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

    let first_diff = bytes_a.iter().zip(&bytes_b).position(|(a, b)| a != b);

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
         and forces a full subset_julia_vm_web relink every time the cache is regenerated. \
         First differing byte: {first_diff:?}."
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
