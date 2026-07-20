use sha2::{Digest, Sha256};
use std::env;
use std::fs;
use std::path::{Path, PathBuf};

const CACHE_BUILD_FINGERPRINT_ROOTS: &[&str] = &[
    "src",
    "../subset_julia_vm/src",
    "../subset_julia_vm_ir/src",
    "../subset_julia_vm_types/src",
    "../subset_julia_vm_bytecode/src",
    "../subset_julia_vm_lowering/src",
];

fn collect_files(path: &Path, files: &mut Vec<PathBuf>) {
    if path.is_file() {
        files.push(path.to_path_buf());
        return;
    }
    let mut entries = fs::read_dir(path)
        .unwrap_or_else(|e| panic!("failed to read {}: {e}", path.display()))
        .map(|entry| entry.expect("directory entry").path())
        .collect::<Vec<_>>();
    entries.sort();
    for entry in entries {
        collect_files(&entry, files);
    }
}

fn fingerprint_roots() -> String {
    let mut files = Vec::new();
    for root in CACHE_BUILD_FINGERPRINT_ROOTS {
        collect_files(Path::new(root), &mut files);
    }
    files.sort();
    let mut hasher = Sha256::new();
    for path in files {
        println!("cargo:rerun-if-changed={}", path.display());
        hasher.update(path.to_string_lossy().as_bytes());
        hasher.update([0]);
        hasher.update(
            fs::read(&path).unwrap_or_else(|e| panic!("failed to read {}: {e}", path.display())),
        );
        hasher.update([0]);
    }
    format!("{:x}", hasher.finalize())
}

fn schema_fingerprint() -> String {
    let manifest = Path::new("src/compile/base_cache_schema_files.txt");
    println!("cargo:rerun-if-changed={}", manifest.display());
    let text = fs::read_to_string(manifest).expect("read Base cache schema manifest");
    let mut files = text
        .lines()
        .filter_map(|line| {
            let line = line.split('#').next().unwrap_or("").trim();
            (!line.is_empty()).then(|| PathBuf::from(line))
        })
        .collect::<Vec<_>>();
    files.sort();
    let mut hasher = Sha256::new();
    for path in files {
        println!("cargo:rerun-if-changed={}", path.display());
        hasher.update(path.to_string_lossy().as_bytes());
        hasher.update([0]);
        hasher.update(
            fs::read(&path).unwrap_or_else(|e| panic!("failed to read {}: {e}", path.display())),
        );
        hasher.update([0]);
    }
    format!("{:x}", hasher.finalize())
}

fn configure_embedded(name: &str, cfg_name: &str, path_env: &str) {
    println!("cargo:rustc-check-cfg=cfg({cfg_name})");
    if let Ok(cache_path) = env::var(name) {
        let path = Path::new(&cache_path);
        assert!(path.exists(), "{name} path does not exist: {cache_path}");
        let absolute = fs::canonicalize(path).expect("canonicalize embedded cache path");
        println!("cargo:rustc-cfg={cfg_name}");
        println!("cargo:rustc-env={path_env}={}", absolute.display());
        println!("cargo:rerun-if-changed={}", absolute.display());
    }
    println!("cargo:rerun-if-env-changed={name}");
}

fn main() {
    println!(
        "cargo:rustc-env=SJULIA_BASE_CACHE_BUILD_HASH={}",
        fingerprint_roots()
    );
    println!(
        "cargo:rustc-env=SJULIA_CACHE_BUILD_FINGERPRINT_ROOTS={}",
        CACHE_BUILD_FINGERPRINT_ROOTS.join(",")
    );
    println!(
        "cargo:rustc-env=SJULIA_BASE_CACHE_SCHEMA_HASH={}",
        schema_fingerprint()
    );
    configure_embedded(
        "SJULIA_BASE_CACHE",
        "has_embedded_base_cache",
        "SJULIA_BASE_CACHE_PATH",
    );
    configure_embedded(
        "SJULIA_PRELOAD_CACHE",
        "has_embedded_preload_cache",
        "SJULIA_PRELOAD_CACHE_PATH",
    );
    configure_embedded(
        "SJULIA_SEEDED_PROGRAM_CACHE",
        "has_embedded_seeded_program_cache",
        "SJULIA_SEEDED_PROGRAM_CACHE_PATH",
    );
    println!("cargo:rerun-if-env-changed=SJULIA_PRELOAD_PACKAGES");
}
