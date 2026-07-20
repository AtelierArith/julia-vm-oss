//! Parser corpus allowlist ratchet (Issue #8637 / #8614).
//!
//! Asserts that the set of `julia/base/**/*.jl` files that the sjulia parser
//! **fails** on is a subset of the allowlist in
//! `docs/vm/PARSER_CORPUS_ALLOWLIST.toml`. A failing file not in the
//! allowlist means a new parser regression; an allowlist entry for a file
//! that now passes means the list should be updated (ratchet tightened).
//!
//! The test gracefully skips if the `julia/` submodule is not checked out.
//! It uses only `julia/base/` (not stdlib/test/) so it stays fast enough for
//! a PR-level nextest run; the full sweep (all three roots) is the nightly
//! path via `scripts/parser_corpus_sweep.sh`.

use std::collections::BTreeSet;
use std::path::PathBuf;
use subset_julia_vm_parser::corpus::{sweep_source, FileOutcome};

/// Parse `docs/vm/PARSER_CORPUS_ALLOWLIST.toml` and return the set of `file`
/// values. Minimal TOML-table parser (no external crate needed) — looks for
/// `file = 'path'` or `file = "path"` lines.
fn load_allowlist_files(allowlist_path: &PathBuf) -> BTreeSet<String> {
    let content = match std::fs::read_to_string(allowlist_path) {
        Ok(c) => c,
        Err(e) => panic!("Cannot read allowlist {allowlist_path:?}: {e}"),
    };
    let mut files = BTreeSet::new();
    for line in content.lines() {
        let trimmed = line.trim();
        if !trimmed.starts_with("file =") {
            continue;
        }
        // file = 'path' or file = "path"
        let rest = trimmed.strip_prefix("file =").unwrap_or("").trim();
        let path = if rest.starts_with('\'') {
            rest.trim_matches('\'')
        } else if rest.starts_with('"') {
            rest.trim_matches('"')
        } else {
            continue;
        };
        files.insert(path.to_string());
    }
    files
}

#[test]
fn parser_corpus_base_ratchet() {
    let manifest_dir = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    let repo_root = manifest_dir.parent().expect("repo root from parser crate");

    // Corpus root for the fast PR-level ratchet: julia/base only.
    let corpus_root = repo_root.join("julia").join("base");
    if !corpus_root.is_dir() {
        // A silent PASS here false-greened a full-suite gate run from a
        // submodule-less worktree (Issue #10946, incident #10935): the run
        // reported green without ever comparing the corpus. Gate contexts
        // export SJULIA_REQUIRE_CORPUS=1 (premerge_gate.sh) so a missing
        // corpus FAILS there; ad-hoc local runs still get the explicit SKIP.
        if std::env::var_os("SJULIA_REQUIRE_CORPUS").is_some() {
            panic!(
                "parser_corpus_base_ratchet: julia/base not found at {:?} and \
                 SJULIA_REQUIRE_CORPUS is set (gate context). Initialize the \
                 submodule (git submodule update --init julia) or symlink \
                 julia/base + julia/stdlib from the main checkout into this \
                 worktree.",
                corpus_root
            );
        }
        eprintln!(
            "SKIP parser_corpus_base_ratchet: julia/base not found at {:?}. \
             Run: git submodule update --init julia",
            corpus_root
        );
        return;
    }

    // Allowlist
    let allowlist_path = repo_root.join("docs/vm/PARSER_CORPUS_ALLOWLIST.toml");
    if !allowlist_path.is_file() {
        panic!(
            "Allowlist not found: {:?}. Re-run scripts/parser_corpus_sweep.sh and \
             review docs/vm/PARSER_CORPUS_ALLOWLIST.toml.",
            allowlist_path
        );
    }
    let allowlist = load_allowlist_files(&allowlist_path);

    // Collect .jl files in julia/base deterministically (sorted).
    let mut jl_files: Vec<PathBuf> = Vec::new();
    fn collect(dir: &PathBuf, out: &mut Vec<PathBuf>) {
        let Ok(entries) = std::fs::read_dir(dir) else {
            return;
        };
        let mut names: Vec<_> = entries.flatten().collect();
        names.sort_by_key(|e| e.path());
        for entry in names {
            let path = entry.path();
            if path.is_dir() {
                collect(&path, out);
            } else if path.extension().and_then(|e| e.to_str()) == Some("jl") {
                out.push(path);
            }
        }
    }
    collect(&corpus_root, &mut jl_files);

    let mut new_failures: Vec<String> = Vec::new(); // failing but not in allowlist
    let mut stale_entries: Vec<String> = Vec::new(); // in allowlist but now passes
    let mut panics: Vec<String> = Vec::new(); // parser panics — bug issues always

    for abs_path in &jl_files {
        let source = match std::fs::read_to_string(abs_path) {
            Ok(s) => s,
            Err(_) => continue, // directories-named-jl etc. — skip
        };
        // Key in the allowlist is the repo-relative path.
        let rel = abs_path
            .strip_prefix(repo_root)
            .expect("path under repo root")
            .to_string_lossy()
            .to_string();

        match sweep_source(&rel, &source) {
            FileOutcome::Ok => {
                if allowlist.contains(&rel) {
                    stale_entries.push(rel);
                }
            }
            FileOutcome::Errors(_) => {
                if !allowlist.contains(&rel) {
                    new_failures.push(rel);
                }
            }
            FileOutcome::Panic(record) => {
                panics.push(format!("{rel}: {}", record.message));
            }
        }
    }

    // Panics are always hard failures (must be filed as `bug` Issues).
    if !panics.is_empty() {
        panic!(
            "PARSER PANICS detected — file `bug` Issues immediately:\n{}",
            panics.join("\n")
        );
    }

    let mut failures = Vec::new();

    // New failures: regressions that need either a fix or an allowlist entry.
    if !new_failures.is_empty() {
        failures.push(format!(
            "NEW corpus divergences not covered by docs/vm/PARSER_CORPUS_ALLOWLIST.toml\n\
             (fix the parser gap or add an allowlist entry with an Issue link):\n{}",
            new_failures
                .iter()
                .map(|s| format!("  {s}"))
                .collect::<Vec<_>>()
                .join("\n")
        ));
    }

    // Stale entries: the parser gap was fixed — tighten the ratchet.
    if !stale_entries.is_empty() {
        failures.push(format!(
            "STALE allowlist entries (files now parse cleanly — remove from allowlist):\n{}",
            stale_entries
                .iter()
                .map(|s| format!("  {s}"))
                .collect::<Vec<_>>()
                .join("\n")
        ));
    }

    if !failures.is_empty() {
        panic!("{}", failures.join("\n\n"));
    }

    // Success
    let ok_count = jl_files.len() - new_failures.len() - stale_entries.len();
    eprintln!(
        "parser_corpus_base_ratchet: {} files checked, {} covered by allowlist",
        jl_files.len(),
        ok_count
    );
}
