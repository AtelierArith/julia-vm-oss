//! Automatic in-suite guard that the committed Base cache schema fingerprint
//! snapshot stays in sync with `CACHE_VERSION` and the schema sources — Issue
//! #10051 slice (Root Cause #1: "Cache schema versioning is manual and drifts
//! silently").
//!
//! # Why this exists
//!
//! `scripts/audit_base_cache_schema_fingerprint.sh` already guards this exact
//! invariant, but it only runs in CI (currently disabled here) and in
//! `scripts/premerge_gate.sh` at merge time. A PR author who bumps
//! `CACHE_VERSION` but forgets `--update` therefore only learns of the drift at
//! merge — and repeatedly does not: #9498 first, then the #10440
//! `CACHE_VERSION` 95→96→97 churn, and again a live 97→98 drift that was red on
//! `main` when this guard was written. Recomputing the fingerprint from a plain
//! `cargo nextest run --lib` turns that silent main-red into an ordinary local
//! test failure that names the exact `--update` command, so the drift surfaces
//! in the normal dev loop and in the full suite the lead certifies — not only
//! when someone remembers to run the bash audit.
//!
//! # What it does NOT change
//!
//! This does not replace the audit or the runtime corruption guard. The bash
//! script stays the single writer of the snapshot (via `--update`), and
//! `deserialize_base_cache` still independently rejects mismatching caches at
//! load time (`docs/vm/CACHE_ARCHITECTURE.md`). This module only *reads* the
//! same three inputs the audit reads — the schema manifest, the schema sources,
//! and `CACHE_VERSION` (parsed from `precompile.rs` source text, exactly as the
//! audit's `sed` does) — and reuses the identical byte-order hashing algorithm,
//! so a green audit and a green test always agree by construction.
//!
//! The module is `#[cfg(test)]` and is intentionally absent from
//! `base_cache_schema_files.txt`: reading `CACHE_VERSION` as source text rather
//! than linking the private const means the guard never has to touch a
//! manifested file, so it cannot perturb the fingerprint it is checking.

// Whole-file test-only (declared `#[cfg(test)] mod schema_fingerprint_guard;`
// in `compile/mod.rs`); this inner allow overrides that ancestor's
// `#![deny(clippy::unwrap_used)]`/`#![deny(clippy::expect_used)]` cascade
// (Issue #10908 Phase 3 of #10869).
#![allow(clippy::unwrap_used, clippy::expect_used)]

use sha2::{Digest, Sha256};
use std::path::Path;

const MANIFEST_REL: &str = "src/compile/base_cache_schema_files.txt";
const SNAPSHOT_REL: &str = "src/compile/base_cache_schema_fingerprint.txt";
const PRECOMPILE_REL: &str = "src/compile/precompile.rs";
const UPDATE_CMD: &str = "bash scripts/audit_base_cache_schema_fingerprint.sh --update";

/// The `subset_julia_vm` crate root, baked in at compile time. Absolute, so the
/// guard is independent of the test runner's working directory.
fn crate_dir() -> &'static Path {
    Path::new(env!("CARGO_MANIFEST_DIR"))
}

fn read_crate_file(rel: &str) -> String {
    let path = crate_dir().join(rel);
    std::fs::read_to_string(&path)
        .unwrap_or_else(|e| panic!("failed to read {}: {e}", path.display()))
}

/// Parse `const CACHE_VERSION: u32 = <n>;` out of `precompile.rs`, mirroring the
/// bash audit's `sed -n 's/^const CACHE_VERSION: u32 = \([0-9]*\);$/\1/p'`.
/// Requires exactly one match so a rename/duplication can't silently pick the
/// wrong value.
fn cache_version_from_source() -> u32 {
    let text = read_crate_file(PRECOMPILE_REL);
    let mut found: Option<u32> = None;
    for line in text.lines() {
        let trimmed = line.trim();
        let Some(rest) = trimmed.strip_prefix("const CACHE_VERSION: u32 = ") else {
            continue;
        };
        let Some(digits) = rest.strip_suffix(';') else {
            continue;
        };
        let value: u32 = digits
            .trim()
            .parse()
            .unwrap_or_else(|_| panic!("could not parse CACHE_VERSION from line: {trimmed:?}"));
        assert!(
            found.replace(value).is_none(),
            "found more than one `const CACHE_VERSION: u32 = ...;` in {PRECOMPILE_REL}"
        );
    }
    found
        .unwrap_or_else(|| panic!("no `const CACHE_VERSION: u32 = ...;` found in {PRECOMPILE_REL}"))
}

/// Parse the schema manifest exactly as the bash audit and `build.rs` do: strip
/// an inline `#` comment, trim, drop blank lines, reject absolute paths.
fn manifest_paths() -> Vec<String> {
    let text = read_crate_file(MANIFEST_REL);
    let mut paths = Vec::new();
    for raw_line in text.lines() {
        let line = raw_line.split('#').next().unwrap_or("").trim();
        if line.is_empty() {
            continue;
        }
        assert!(
            !Path::new(line).is_absolute(),
            "schema manifest paths must be relative: {line}"
        );
        paths.push(line.to_string());
    }
    paths
}

/// Recompute the schema fingerprint using the identical algorithm to
/// `scripts/audit_base_cache_schema_fingerprint.sh` (and thus to what `--update`
/// writes into the snapshot): sort the manifest path strings in byte order
/// (`Vec<String>::sort()` == `LC_ALL=C sort`), then hash `<path>\0<file-bytes>\0`
/// for each, concatenated, with SHA-256. Because the algorithm matches the
/// writer, a passing bash audit and a passing test never disagree.
fn recompute_fingerprint() -> String {
    let mut paths = manifest_paths();
    paths.sort();
    let mut hasher = Sha256::new();
    for rel in &paths {
        let path = crate_dir().join(rel);
        let bytes = std::fs::read(&path)
            .unwrap_or_else(|e| panic!("failed to read schema source {}: {e}", path.display()));
        hasher.update(rel.as_bytes());
        hasher.update([0u8]);
        hasher.update(&bytes);
        hasher.update([0u8]);
    }
    format!("{:x}", hasher.finalize())
}

/// Parse the two-line snapshot into `(CACHE_VERSION, SCHEMA_FINGERPRINT)`.
fn parse_snapshot(text: &str) -> (Option<u32>, Option<String>) {
    let mut version = None;
    let mut fingerprint = None;
    for line in text.lines() {
        if let Some(v) = line.strip_prefix("CACHE_VERSION=") {
            version = v.trim().parse::<u32>().ok();
        } else if let Some(fp) = line.strip_prefix("SCHEMA_FINGERPRINT=") {
            fingerprint = Some(fp.trim().to_string());
        }
    }
    (version, fingerprint)
}

/// The pure checker: compare the live (`current_version`, `current_fingerprint`)
/// against a snapshot's text, returning a human-readable, `--update`-pointing
/// error on any drift. Factored out so the negative self-test can feed it a
/// deliberately stale snapshot without touching the real committed file.
fn check_snapshot(
    current_version: u32,
    current_fingerprint: &str,
    snapshot_text: &str,
) -> Result<(), String> {
    let (snap_version, snap_fingerprint) = parse_snapshot(snapshot_text);
    let snap_version = snap_version
        .ok_or_else(|| format!("snapshot missing CACHE_VERSION=<n>; run: {UPDATE_CMD}"))?;
    let snap_fingerprint = snap_fingerprint.ok_or_else(|| {
        format!("snapshot missing SCHEMA_FINGERPRINT=<sha256>; run: {UPDATE_CMD}")
    })?;
    if current_version != snap_version {
        return Err(format!(
            "Base cache schema snapshot CACHE_VERSION is stale (current {current_version}, \
             snapshot {snap_version}). Run: {UPDATE_CMD}"
        ));
    }
    if current_fingerprint != snap_fingerprint {
        return Err(format!(
            "Base cache schema fingerprint changed (current {current_fingerprint}, snapshot \
             {snap_fingerprint}). A file listed in {MANIFEST_REL} changed; bump CACHE_VERSION in \
             {PRECOMPILE_REL}, then run: {UPDATE_CMD}"
        ));
    }
    Ok(())
}

/// The guard: the committed snapshot must match the live `CACHE_VERSION` and the
/// recomputed fingerprint. FAILS in `cargo nextest run --lib` when a schema
/// source or `CACHE_VERSION` moved without a matching `--update` — the exact
/// silent-main-red that Issue #10051 Root Cause #1 keeps producing.
#[test]
fn base_cache_schema_snapshot_is_in_sync() {
    let snapshot_text = read_crate_file(SNAPSHOT_REL);
    let recomputed = recompute_fingerprint();
    let version = cache_version_from_source();
    if let Err(msg) = check_snapshot(version, &recomputed, &snapshot_text) {
        panic!(
            "Base cache schema fingerprint snapshot is out of sync (Issue #10051):\n  {msg}\n\
             (snapshot file: {})",
            crate_dir().join(SNAPSHOT_REL).display()
        );
    }
}

/// NEGATIVE self-test (per docs/vm/CODE_AUDITS.md "Adding a New Audit Script"):
/// prove this guard actually FAILS on a stale snapshot instead of silently
/// passing. Feeds the checker deliberately stale inputs — one with a wrong
/// fingerprint, one with a wrong `CACHE_VERSION` — and requires each to be
/// rejected with the `--update` guidance. Uses the SAME `check_snapshot` the
/// positive test uses, so it exercises the real detection path; the final
/// assertion confirms the matching snapshot is still accepted, so the failures
/// above are caused by the injected staleness rather than a checker that
/// rejects everything.
#[test]
fn stale_snapshot_is_detected() {
    let recomputed = recompute_fingerprint();
    let version = cache_version_from_source();

    // 1. Correct version, wrong fingerprint.
    let stale_fp = format!(
        "CACHE_VERSION={version}\nSCHEMA_FINGERPRINT={}\n",
        "0".repeat(64)
    );
    let err = check_snapshot(version, &recomputed, &stale_fp)
        .expect_err("a stale fingerprint MUST be detected");
    assert!(
        err.contains("fingerprint changed") && err.contains(UPDATE_CMD),
        "stale-fingerprint error must name the fix command, got: {err}"
    );

    // 2. Correct fingerprint, wrong (off-by-one) version.
    let stale_ver = format!(
        "CACHE_VERSION={}\nSCHEMA_FINGERPRINT={recomputed}\n",
        version.wrapping_add(1)
    );
    let err = check_snapshot(version, &recomputed, &stale_ver)
        .expect_err("a stale CACHE_VERSION MUST be detected");
    assert!(
        err.contains("CACHE_VERSION is stale") && err.contains(UPDATE_CMD),
        "stale-version error must name the fix command, got: {err}"
    );

    // Positive control: the matching snapshot is accepted.
    let fresh = format!("CACHE_VERSION={version}\nSCHEMA_FINGERPRINT={recomputed}\n");
    check_snapshot(version, &recomputed, &fresh).expect("the matching snapshot must be accepted");
}

/// Machine-check the ordering "coincidence" the audit script's header flags as
/// unproven (Issue #10051 slice A): the byte-order fingerprint this guard and
/// the bash audit compute must equal the `Vec<PathBuf>::sort()`-order hash
/// `build.rs` embeds as `SJULIA_BASE_CACHE_SCHEMA_HASH` (exposed via the public
/// `base_cache_schema_fingerprint()`). If a future manifest path makes
/// `LC_ALL=C` byte order and Rust `Path` component order diverge (e.g. a
/// `value.rs` sibling of a `value/` directory), this fails loudly instead of
/// letting the committed snapshot and the runtime-embedded hash drift apart
/// unnoticed — the "possible future strengthening" the audit header left open.
#[test]
fn recomputed_fingerprint_matches_build_rs_embedded_hash() {
    assert_eq!(
        recompute_fingerprint(),
        super::precompile::base_cache_schema_fingerprint(),
        "byte-order (bash/snapshot) and Path-order (build.rs SJULIA_BASE_CACHE_SCHEMA_HASH) \
         fingerprints diverged; a manifest path now sorts differently between the two rules. \
         Reconcile scripts/audit_base_cache_schema_fingerprint.sh and build.rs before relying on \
         the snapshot (Issue #10051)."
    );
}
