//! Build script to generate fixture tests from manifest.toml files
//!
//! This generates individual test functions for each test case in manifest.toml files,
//! grouped by category. Supports both:
//! - Single root manifest.toml (legacy mode)
//! - Distributed manifest.toml files in each category directory
//!
//! When distributed manifests exist, they are merged with the root manifest.

// Build scripts should panic on errors (standard Rust build script pattern)
#![allow(clippy::unwrap_used)]
#![allow(clippy::expect_used)]

use serde::Deserialize;
use sha2::{Digest, Sha256};
use std::collections::BTreeMap;
use std::env;
use std::fs;
use std::path::{Path, PathBuf};

const FIXTURE_BATCH_SIZE: usize = 32;

/// Root manifest structure (contains global config and optionally tests)
///
/// `deny_unknown_fields` (Issue #9486): an unknown-key typo such as `[[test]]`
/// instead of `[[tests]]` is valid TOML, and serde's default is to silently
/// ignore unknown keys — the typo'd entry would be silently deregistered while
/// every layer stayed green. Rejecting unknown fields turns the whole typo
/// class into a loud build failure naming the manifest. Keep the manifest
/// structs here and in `tests/fixture_tests.rs` in sync.
#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct RootManifest {
    #[allow(dead_code)]
    config: Config,
    #[serde(default)]
    tests: Vec<TestCase>,
}

/// Category manifest structure (tests only, no config)
#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct CategoryManifest {
    #[serde(default)]
    tests: Vec<TestCase>,
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct Config {
    #[allow(dead_code)]
    epsilon: f64,
}

/// Expected value can be a float, boolean, or string
#[allow(dead_code)]
#[derive(Debug, Deserialize)]
#[serde(untagged)]
enum Expected {
    Bool(bool),
    Float(f64),
    String(String),
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct TestCase {
    name: String,
    file: String,
    #[allow(dead_code)]
    expected: Expected,
    #[serde(default)]
    #[allow(dead_code)]
    description: String,
    #[serde(default)]
    skip: bool,
    /// Per-test environment variables; consumed by the runtime harness in
    /// `tests/fixture_tests.rs` (declared here so `deny_unknown_fields`
    /// accepts it, Issue #9486).
    #[serde(default)]
    #[allow(dead_code)]
    env: BTreeMap<String, String>,
    /// Marks a fixture that is an intentional SubsetJuliaVM extension and must
    /// NOT be run under upstream `julia` for parity (e.g. callable GlobalRef,
    /// Issue #302). Declarative metadata for parity tooling; declared so
    /// `deny_unknown_fields` accepts it (Issue #9486).
    #[serde(default)]
    #[allow(dead_code)]
    skip_julia_test: bool,
    /// Marks a fixture whose semantics depend on the compile/persistent cache
    /// mode (GC/WeakRef/finalizer, struct-table identity across cache restore —
    /// the #10092 bug class). Categories containing a `cache_sensitive = true`
    /// entry are run under BOTH cache modes by
    /// `scripts/check_cache_sensitive_fixture_lane.sh` (Issue #10223).
    /// Declarative metadata for that lane; declared so `deny_unknown_fields`
    /// accepts it (Issue #9486).
    #[serde(default)]
    #[allow(dead_code)]
    cache_sensitive: bool,
}

fn sanitize_mod_name(name: &str) -> String {
    let sanitized = name.replace(['-', '.', ' '], "_");

    // Handle Rust reserved keywords (strict, reserved, and weak)
    match sanitized.as_str() {
        "abstract" => "abstract_tests".to_string(),
        "type" => "type_tests".to_string(),
        "types" => "types_tests".to_string(),
        "struct" => "struct_tests".to_string(),
        "where" => "where_tests".to_string(),
        "mod" => "mod_tests".to_string(),
        "module" => "module_tests".to_string(),
        "fn" => "fn_tests".to_string(),
        "function" => "function_tests".to_string(),
        "loop" => "loop_tests".to_string(),
        "for" => "for_tests".to_string(),
        "while" => "while_tests".to_string(),
        "if" => "if_tests".to_string(),
        "else" => "else_tests".to_string(),
        "match" => "match_tests".to_string(),
        "return" => "return_tests".to_string(),
        "break" => "break_tests".to_string(),
        "continue" => "continue_tests".to_string(),
        "const" => "const_tests".to_string(),
        "static" => "static_tests".to_string(),
        "mut" => "mut_tests".to_string(),
        "ref" => "ref_tests".to_string(),
        "self" => "self_tests".to_string(),
        "super" => "super_tests".to_string(),
        "crate" => "crate_tests".to_string(),
        "impl" => "impl_tests".to_string(),
        "trait" => "trait_tests".to_string(),
        "enum" => "enum_tests".to_string(),
        "union" => "union_tests".to_string(),
        "unsafe" => "unsafe_tests".to_string(),
        "async" => "async_tests".to_string(),
        "await" => "await_tests".to_string(),
        "dyn" => "dyn_tests".to_string(),
        "move" => "move_tests".to_string(),
        "pub" => "pub_tests".to_string(),
        "use" => "use_tests".to_string(),
        "extern" => "extern_tests".to_string(),
        "let" => "let_tests".to_string(),
        "box" => "box_tests".to_string(),
        "final" => "final_tests".to_string(),
        "override" => "override_tests".to_string(),
        "priv" => "priv_tests".to_string(),
        "virtual" => "virtual_tests".to_string(),
        "yield" => "yield_tests".to_string(),
        "become" => "become_tests".to_string(),
        "do" => "do_tests".to_string(),
        "macro" => "macro_tests".to_string(),
        "typeof" => "typeof_tests".to_string(),
        "unsized" => "unsized_tests".to_string(),
        "try" => "try_tests".to_string(),
        _ => sanitized,
    }
}

/// Load all test cases from root manifest and distributed category manifests
fn load_all_tests(fixtures_dir: &Path) -> Vec<TestCase> {
    let mut all_tests = Vec::new();

    // 1. Load root manifest (required for config, may contain tests)
    let root_manifest_path = fixtures_dir.join("manifest.toml");
    println!("cargo:rerun-if-changed={}", root_manifest_path.display());

    let root_content =
        fs::read_to_string(&root_manifest_path).expect("Failed to read root manifest.toml");
    let root_manifest: RootManifest =
        toml::from_str(&root_content).expect("Failed to parse root manifest.toml");

    // Add tests from root manifest (legacy support)
    all_tests.extend(root_manifest.tests);

    // 2. Scan for category manifest.toml files
    if let Ok(entries) = fs::read_dir(fixtures_dir) {
        for entry in entries.filter_map(|e| e.ok()) {
            let path = entry.path();
            if path.is_dir() {
                let category_manifest_path = path.join("manifest.toml");
                if category_manifest_path.exists() {
                    // Tell Cargo to rerun if this manifest changes
                    println!(
                        "cargo:rerun-if-changed={}",
                        category_manifest_path.display()
                    );

                    let category_name = path
                        .file_name()
                        .and_then(|n| n.to_str())
                        .unwrap_or("unknown");

                    let content = fs::read_to_string(&category_manifest_path).unwrap_or_else(|e| {
                        panic!(
                            "Failed to read category manifest {}: {}",
                            category_manifest_path.display(),
                            e
                        )
                    });
                    // Loud manifest failure (Issue #9378): a malformed category
                    // manifest previously only printed a warning here and then
                    // generated ZERO tests for the whole category, so a dropped
                    // `[[tests]]` header (e.g. from a botched merge resolution)
                    // silently deleted the category's coverage while the full
                    // suite stayed green. Fail the build instead — a green build
                    // must never be reachable with a manifest the harness cannot
                    // parse.
                    match toml::from_str::<CategoryManifest>(&content) {
                        Ok(category_manifest) => {
                            // Prefix file paths with category name
                            for mut test in category_manifest.tests {
                                // If file doesn't contain '/', prefix with category
                                if !test.file.contains('/') {
                                    test.file = format!("{}/{}", category_name, test.file);
                                }
                                all_tests.push(test);
                            }
                        }
                        Err(e) => {
                            panic!(
                                "Failed to parse category manifest {} (Issue #9378 — a malformed \
                                 manifest must fail the build loudly, not silently drop the \
                                 category's fixture coverage): {}",
                                category_manifest_path.display(),
                                e
                            );
                        }
                    }
                }
            }
        }
    }

    all_tests
}

fn collect_cache_fingerprint_files(root: &Path, files: &mut Vec<PathBuf>) {
    let entries = fs::read_dir(root).unwrap_or_else(|e| {
        panic!(
            "Failed to read cache fingerprint dir {}: {e}",
            root.display()
        )
    });
    for entry in entries {
        let entry = entry.unwrap_or_else(|e| {
            panic!(
                "Failed to read cache fingerprint entry in {}: {e}",
                root.display()
            )
        });
        let path = entry.path();
        if path.is_dir() {
            collect_cache_fingerprint_files(&path, files);
        } else if path.extension().is_some_and(|ext| ext == "rs") {
            files.push(path);
        }
    }
}

/// The workspace `target/` directory, mirroring
/// `pipeline::workspace_target_dir()`'s resolution so this build script finds
/// the exact same persistent prelude cache files that runtime writes there
/// (Issue #10123).
fn workspace_target_dir_for_prelude_cache_scan() -> PathBuf {
    if let Ok(target_dir) = env::var("CARGO_TARGET_DIR") {
        return PathBuf::from(target_dir);
    }
    Path::new(&env::var("CARGO_MANIFEST_DIR").expect("CARGO_MANIFEST_DIR not set"))
        .parent()
        .map(|p| p.join("target"))
        .unwrap_or_else(|| PathBuf::from("target"))
}

/// Find the most-recently-modified `sjulia_prelude_program_*.bin` in the
/// workspace target directory, if any (Issue #10123). Picking a stale one is
/// harmless: the runtime load path validates it and falls back to parsing
/// from source on any mismatch.
fn find_newest_persistent_prelude_cache() -> Option<PathBuf> {
    let dir = workspace_target_dir_for_prelude_cache_scan();
    // Watch the directory itself, unconditionally, EVEN WHEN no matching
    // file exists yet (Issue #10216 review): the loop below only registers
    // `rerun-if-changed` for files it actually finds, so on a clean checkout
    // (no `sjulia_prelude_program_*.bin` present yet) nothing gets watched
    // at all. Without this, the natural "build, run sjulia once to create
    // the persistent cache, build again" flow this feature exists for never
    // triggers a build.rs re-run on that second build — Cargo has no
    // registered input that changed, so it considers the build fresh, and
    // the newly-created cache silently isn't picked up until some UNRELATED
    // source edit happens to force a rerun. A directory's own mtime changes
    // when a file is added/removed inside it, so this one registration is
    // enough to make Cargo re-run build.rs (and thus re-scan) as soon as the
    // first persistent cache file appears.
    println!("cargo:rerun-if-changed={}", dir.display());
    let entries = fs::read_dir(&dir).ok()?;

    let mut newest: Option<(std::time::SystemTime, PathBuf)> = None;
    for entry in entries.filter_map(Result::ok) {
        let path = entry.path();
        let Some(name) = path.file_name().and_then(|n| n.to_str()) else {
            continue;
        };
        if !(name.starts_with("sjulia_prelude_program_") && name.ends_with(".bin")) {
            continue;
        }
        let Ok(metadata) = entry.metadata() else {
            continue;
        };
        let Ok(modified) = metadata.modified() else {
            continue;
        };
        // Re-run if this candidate file's content changes, so a fresher
        // persistent cache (e.g. after a Base source edit invalidates the
        // hash and a new file appears) is picked up on the next build.
        println!("cargo:rerun-if-changed={}", path.display());
        let is_newer = newest.as_ref().is_none_or(|(t, _)| modified > *t);
        if is_newer {
            newest = Some((modified, path));
        }
    }
    newest.map(|(_, path)| path)
}

/// Source roots hashed into `SJULIA_BASE_CACHE_BUILD_HASH` (Issue #10332).
///
/// Serialized cache payloads (Base cache, prelude `Program` cache, `.sjvmbc`)
/// carry serde-derived types from the sibling crates below (`Program`/`Expr`/
/// `JuliaType`/`TypeExpr`/`TypeParam` from `subset_julia_vm_types`,
/// `CompiledProgram`/`Instr`/`Value` from `subset_julia_vm_bytecode`, `Span`
/// from `subset_julia_vm_ir`). bincode/postcard are positional, so a
/// serde-shape change in ANY of these crates changes the wire meaning; the
/// build fingerprint must therefore cover every crate whose types are
/// reachable from a serialized payload, not just `subset_julia_vm/src` —
/// otherwise a stale cache passes the fingerprint check and is misdecoded
/// (historically only guarded by manual CACHE_VERSION bumps, e.g. the
/// `Expr::Convert` bump to 93). Exposed to tests via the
/// `SJULIA_CACHE_BUILD_FINGERPRINT_ROOTS` env so coverage regressions fail a
/// unit test instead of silently narrowing invalidation.
const CACHE_BUILD_FINGERPRINT_ROOTS: &[&str] = &[
    "src",
    "../subset_julia_vm_compile/src",
    "../subset_julia_vm_lowering/src",
    "../subset_julia_vm_vm/src",
    "../subset_julia_vm_ir/src",
    "../subset_julia_vm_types/src",
    "../subset_julia_vm_bytecode/src",
];

fn base_cache_build_fingerprint() -> String {
    let mut files = Vec::new();
    for root in CACHE_BUILD_FINGERPRINT_ROOTS {
        collect_cache_fingerprint_files(Path::new(root), &mut files);
    }
    files.sort();

    let mut hasher = Sha256::new();
    for path in files {
        println!("cargo:rerun-if-changed={}", path.display());
        hasher.update(path.to_string_lossy().as_bytes());
        hasher.update([0]);
        let bytes = fs::read(&path).unwrap_or_else(|e| {
            panic!(
                "Failed to read {} for cache fingerprint: {e}",
                path.display()
            )
        });
        hasher.update(bytes);
        hasher.update([0]);
    }
    format!("{:x}", hasher.finalize())
}

fn base_cache_schema_fingerprint() -> String {
    let manifest = Path::new("../subset_julia_vm_compile/src/compile/base_cache_schema_files.txt");
    println!("cargo:rerun-if-changed={}", manifest.display());

    let manifest_text = fs::read_to_string(manifest).unwrap_or_else(|e| {
        panic!(
            "Failed to read Base cache schema manifest {}: {e}",
            manifest.display()
        )
    });
    let mut files = Vec::new();
    for (line_index, raw_line) in manifest_text.lines().enumerate() {
        let line = raw_line.split('#').next().unwrap_or("").trim();
        if line.is_empty() || line.starts_with('#') {
            continue;
        }
        let path = Path::new("../subset_julia_vm_compile").join(line);
        if path.is_absolute() {
            panic!(
                "Base cache schema manifest {} line {} must be relative, got {}",
                manifest.display(),
                line_index + 1,
                line
            );
        }
        files.push(path);
    }
    files.sort();

    let mut hasher = Sha256::new();
    for path in files {
        println!("cargo:rerun-if-changed={}", path.display());
        hasher.update(path.to_string_lossy().as_bytes());
        hasher.update([0]);
        let bytes = fs::read(&path).unwrap_or_else(|e| {
            panic!(
                "Failed to read {} for Base cache schema fingerprint: {e}",
                path.display()
            )
        });
        hasher.update(bytes);
        hasher.update([0]);
    }
    format!("{:x}", hasher.finalize())
}

fn main() {
    let base_cache_build_hash = base_cache_build_fingerprint();
    println!("cargo:rustc-env=SJULIA_BASE_CACHE_BUILD_HASH={base_cache_build_hash}");
    // Coverage contract for unit tests (Issue #10332): which source roots the
    // build fingerprint hashes. See CACHE_BUILD_FINGERPRINT_ROOTS.
    println!(
        "cargo:rustc-env=SJULIA_CACHE_BUILD_FINGERPRINT_ROOTS={}",
        CACHE_BUILD_FINGERPRINT_ROOTS.join(",")
    );
    let base_cache_schema_hash = base_cache_schema_fingerprint();
    println!("cargo:rustc-env=SJULIA_BASE_CACHE_SCHEMA_HASH={base_cache_schema_hash}");

    let fixtures_dir = Path::new("tests/fixtures");

    // Load all tests from root and distributed manifests
    let all_tests = load_all_tests(fixtures_dir);

    // Detect duplicate test names at build time (Issue #3135, #3138).
    // `run_fixture_test` uses `iter().find()` which returns the first match,
    // so duplicate names would silently load the wrong file. Fail fast here.
    {
        let mut seen: std::collections::HashMap<&str, &str> = std::collections::HashMap::new();
        for test in &all_tests {
            if let Some(prev_file) = seen.insert(test.name.as_str(), test.file.as_str()) {
                panic!(
                    "build.rs: duplicate fixture test name '{}'\n  first:  {}\n  second: {}\n\
                     Test names must be unique across all categories.\n\
                     Tip: prefix the name with the category (e.g. 'meta_isidentifier_validation').",
                    test.name, prev_file, test.file
                );
            }
        }
    }

    // Group tests by category (first part of file path)
    let mut categories: BTreeMap<String, Vec<TestCase>> = BTreeMap::new();

    for test in all_tests {
        let category = test.file.split('/').next().unwrap_or("misc").to_string();
        categories.entry(category).or_default().push(test);
    }

    // Generate the test code
    let mut code = String::new();

    code.push_str("// Auto-generated by build.rs - DO NOT EDIT\n");
    code.push_str("// Generated from tests/fixtures/**/manifest.toml\n\n");

    // Generate category modules.
    //
    // nextest executes every Rust test as a separate process. Emitting one test
    // per fixture made the full release suite reload the Base cache thousands
    // of times, while a single test per large category serialized too much work.
    // Keep category-level targeting while batching fixture cases into moderate
    // chunks so each process reuses the thread-local Base cache without blocking
    // the full suite behind one long-running category.
    for (category, tests) in &categories {
        let mod_name = sanitize_mod_name(category);

        code.push_str(&format!("mod {} {{\n", mod_name));
        code.push_str("    use super::*;\n\n");

        for (chunk_index, chunk) in tests.chunks(FIXTURE_BATCH_SIZE).enumerate() {
            code.push_str("    #[test]\n");
            if chunk.iter().all(|test| test.skip) {
                code.push_str("    #[ignore]\n");
            }
            code.push_str(&format!("    fn chunk_{:03}() {{\n", chunk_index));
            code.push_str("        run_fixture_category(&[\n");
            for test in chunk {
                code.push_str(&format!("            \"{}\",\n", test.name));
            }
            code.push_str("        ]);\n");
            code.push_str("    }\n\n");
        }

        code.push_str("}\n\n");
    }

    // Write the generated code
    let out_dir = env::var("OUT_DIR").expect("OUT_DIR not set");
    let dest_path = Path::new(&out_dir).join("fixture_tests_generated.rs");
    fs::write(&dest_path, code).expect("Failed to write generated tests");

    // Handle embedded Base cache for precompiled bytecode (Issue #2929)
    println!("cargo:rustc-check-cfg=cfg(has_embedded_base_cache)");
    if let Ok(cache_path) = env::var("SJULIA_BASE_CACHE") {
        let path = Path::new(&cache_path);
        if path.exists() {
            let abs_path =
                fs::canonicalize(path).expect("Failed to canonicalize SJULIA_BASE_CACHE path");
            println!("cargo:rustc-cfg=has_embedded_base_cache");
            println!(
                "cargo:rustc-env=SJULIA_BASE_CACHE_PATH={}",
                abs_path.display()
            );
            println!("cargo:rerun-if-changed={}", abs_path.display());
        } else {
            panic!("SJULIA_BASE_CACHE path does not exist: {}", cache_path);
        }
    }
    println!("cargo:rerun-if-env-changed=SJULIA_BASE_CACHE");

    // Opportunistically auto-discover an already-computed persistent prelude
    // Program cache for `--release` builds (Issue #10123), so a SECOND (and
    // every later) local release rebuild embeds it without the manual
    // `--precompile-prelude` + `SJULIA_PRELUDE_PROGRAM_CACHE=...` two-step
    // dance documented in AGENTS.md.
    //
    // This does NOT help the very first release build from a clean checkout
    // (nothing has been computed yet) — a build script cannot invoke the
    // crate's own `Parser`/`Lowering` to generate one itself, since
    // `subset_julia_vm`'s build.rs cannot depend on `subset_julia_vm` (that
    // would be a circular self-dependency). The persistent cache is instead
    // populated as a side effect of the FIRST `sjulia` run against a given
    // source-hash (pipeline.rs's normal lazy-load path already writes
    // `target/sjulia_prelude_program_<hash>.bin` there), so this only starts
    // paying off from the next release build onward.
    //
    // Gated to `--release` only (`PROFILE=release`) per the issue's own
    // tradeoff analysis: this is a directory scan plus a `rerun-if-changed`
    // registration, so it must not add overhead to the inner dev-build loop.
    // Skipped entirely when the developer already set
    // `SJULIA_PRELUDE_PROGRAM_CACHE` explicitly (manual override wins).
    //
    // Auto-picking the wrong (stale/foreign) file is safe by construction:
    // `pipeline::deserialize_prelude_program` independently validates
    // version/source-hash/enum-fingerprint at load time and falls back to
    // parsing from source whenever it doesn't match — exactly like today
    // when no cache is embedded at all.
    if env::var_os("SJULIA_PRELUDE_PROGRAM_CACHE").is_none()
        && env::var("PROFILE").as_deref() == Ok("release")
    {
        if let Some(auto_path) = find_newest_persistent_prelude_cache() {
            println!(
                "cargo:warning=Issue #10123: auto-embedding persistent prelude cache {}",
                auto_path.display()
            );
            // SAFETY: build scripts run single-threaded before any other code
            // in this crate observes the environment, so there is no
            // concurrent reader to race with.
            #[allow(unused_unsafe)]
            unsafe {
                env::set_var("SJULIA_PRELUDE_PROGRAM_CACHE", &auto_path);
            }
        }
    }

    // Handle embedded prelude Program cache for parse/lower cold-start reduction (Issue #6026)
    println!("cargo:rustc-check-cfg=cfg(has_embedded_prelude_program)");
    if let Ok(cache_path) = env::var("SJULIA_PRELUDE_PROGRAM_CACHE") {
        let path = Path::new(&cache_path);
        if path.exists() {
            let abs_path = fs::canonicalize(path)
                .expect("Failed to canonicalize SJULIA_PRELUDE_PROGRAM_CACHE path");
            println!("cargo:rustc-cfg=has_embedded_prelude_program");
            println!(
                "cargo:rustc-env=SJULIA_PRELUDE_PROGRAM_CACHE_PATH={}",
                abs_path.display()
            );
            println!("cargo:rerun-if-changed={}", abs_path.display());
        } else {
            panic!(
                "SJULIA_PRELUDE_PROGRAM_CACHE path does not exist: {}",
                cache_path
            );
        }
    }
    println!("cargo:rerun-if-env-changed=SJULIA_PRELUDE_PROGRAM_CACHE");

    // Handle embedded preloaded-package bytecode cache (Issue #9189/#9230). iOS
    // has no writable disk for the persistent-file tier (`loader.rs`), so the
    // whole-closure preload cache must be embedded via `include_bytes!` the same
    // way the Base cache is.
    println!("cargo:rustc-check-cfg=cfg(has_embedded_preload_cache)");
    if let Ok(cache_path) = env::var("SJULIA_PRELOAD_CACHE") {
        let path = Path::new(&cache_path);
        if path.exists() {
            let abs_path =
                fs::canonicalize(path).expect("Failed to canonicalize SJULIA_PRELOAD_CACHE path");
            println!("cargo:rustc-cfg=has_embedded_preload_cache");
            println!(
                "cargo:rustc-env=SJULIA_PRELOAD_CACHE_PATH={}",
                abs_path.display()
            );
            println!("cargo:rerun-if-changed={}", abs_path.display());
        } else {
            panic!("SJULIA_PRELOAD_CACHE path does not exist: {}", cache_path);
        }
    }
    println!("cargo:rerun-if-env-changed=SJULIA_PRELOAD_CACHE");
    println!("cargo:rerun-if-env-changed=SJULIA_PRELOAD_PACKAGES");

    // Handle embedded seeded-program cache (Issue #10120): a small, fixed
    // list of common short programs (`println("Hello World")`, ...)
    // precompiled at build time via `sjulia --precompile-seeded <path>` and
    // embedded the same way as the Base/prelude/preload caches above, so
    // `PROGRAM_CACHE` (compile/cache.rs) is pre-populated with them before
    // the first real compile in a process.
    println!("cargo:rustc-check-cfg=cfg(has_embedded_seeded_program_cache)");
    if let Ok(cache_path) = env::var("SJULIA_SEEDED_PROGRAM_CACHE") {
        let path = Path::new(&cache_path);
        if path.exists() {
            let abs_path = fs::canonicalize(path)
                .expect("Failed to canonicalize SJULIA_SEEDED_PROGRAM_CACHE path");
            println!("cargo:rustc-cfg=has_embedded_seeded_program_cache");
            println!(
                "cargo:rustc-env=SJULIA_SEEDED_PROGRAM_CACHE_PATH={}",
                abs_path.display()
            );
            println!("cargo:rerun-if-changed={}", abs_path.display());
        } else {
            panic!(
                "SJULIA_SEEDED_PROGRAM_CACHE path does not exist: {}",
                cache_path
            );
        }
    }
    println!("cargo:rerun-if-env-changed=SJULIA_SEEDED_PROGRAM_CACHE");
}
