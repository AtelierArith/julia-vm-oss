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
use std::collections::BTreeMap;
use std::env;
use std::fs;
use std::path::Path;

/// Root manifest structure (contains global config and optionally tests)
#[derive(Debug, Deserialize)]
struct RootManifest {
    #[allow(dead_code)]
    config: Config,
    #[serde(default)]
    tests: Vec<TestCase>,
}

/// Category manifest structure (tests only, no config)
#[derive(Debug, Deserialize)]
struct CategoryManifest {
    #[serde(default)]
    tests: Vec<TestCase>,
}

#[derive(Debug, Deserialize)]
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

fn sanitize_test_name(name: &str) -> String {
    name.replace(['-', '.', ' '], "_")
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

                    if let Ok(content) = fs::read_to_string(&category_manifest_path) {
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
                                eprintln!(
                                    "Warning: Failed to parse {}: {}",
                                    category_manifest_path.display(),
                                    e
                                );
                            }
                        }
                    }
                }
            }
        }
    }

    all_tests
}

fn main() {
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

    // Generate category modules
    for (category, tests) in &categories {
        let mod_name = sanitize_mod_name(category);

        code.push_str(&format!("mod {} {{\n", mod_name));
        code.push_str("    use super::*;\n\n");

        // Generate individual test functions
        for test in tests {
            let test_fn_name = sanitize_test_name(&test.name);
            code.push_str("    #[test]\n");
            // Add #[ignore] attribute for skipped tests
            if test.skip {
                code.push_str("    #[ignore]\n");
            }
            code.push_str(&format!("    fn {}() {{\n", test_fn_name));
            code.push_str(&format!("        run_fixture_test(\"{}\");\n", test.name));
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
            panic!(
                "SJULIA_BASE_CACHE path does not exist: {}",
                cache_path
            );
        }
    }
    println!("cargo:rerun-if-env-changed=SJULIA_BASE_CACHE");
}
