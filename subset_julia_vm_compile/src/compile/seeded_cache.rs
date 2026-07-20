//! Seeded `PROGRAM_CACHE` entries for common short programs (Issue #10120).
//!
//! `PROGRAM_CACHE` (`cache.rs`) memoizes whole-program `CompiledProgram`s by
//! hash, but starts empty every process, so a common one-shot program like
//! `println("Hello World")` always pays a full compile the first (and, per
//! Issue #6348's one-shot-CLI tradeoff, only starts benefiting from caching
//! on its SECOND) run of a fresh process.
//!
//! This precompiles a small, fixed list of common short programs at BUILD
//! TIME (`sjulia --precompile-seeded <path>`, mirroring
//! `--precompile-base`/`--precompile-prelude`) and, when the resulting bytes
//! are embedded (`SJULIA_SEEDED_PROGRAM_CACHE`, wired the same way as
//! `SJULIA_BASE_CACHE`/`SJULIA_PRELUDE_PROGRAM_CACHE` in `build.rs`), makes
//! them available to `PROGRAM_CACHE`'s lookup before the first real compile
//! in a process — so a matching program hits `PROGRAM_CACHE` on its FIRST
//! compile too, not just its second.
//!
//! Each entry's `CompiledProgram` is itself a full Base-merged compile (a few
//! MB), so with several seeds embedded, EAGERLY decoding every entry up front
//! would cost several times a single Base-cache decode -- more than the
//! decode Issue #10118 just optimized, and more than the compile this
//! feature is supposed to let a matching program skip. So the outer
//! [`SeededCache`] envelope stores each entry's `CompiledProgram` as an
//! UNDECODED byte blob (`SeededEntryRaw`); only the ONE entry whose hash
//! actually matches the program being compiled is decoded, on demand, by
//! `cache.rs`'s lookup.
//!
//! Depends on the Issue #10118 postcard migration: this reuses
//! `precompile::cache_serialize`/`cache_deserialize`, so it inherits the same
//! wire format and stale-cache-rejection discipline (below).

// Issue #10906 (Phase 1c of #10869): seeded PROGRAM_CACHE cache-load
// boundary — zero real unwrap_used/expect_used/panic sites in production
// code (every match is inside the cfg(test) module, which carries an
// explicit allow).
#![deny(clippy::unwrap_used)]
#![deny(clippy::expect_used)]

use crate::bytecode::CompiledProgram;

/// Common short programs worth precompiling at build time. Kept intentionally
/// small: each entry costs real build time (a full parse + lower + compile)
/// and a little binary size, so this is not a general cache-warming
/// mechanism — just the handful of trivial "hello world"-class programs a
/// REPL / first-run / iOS Run-button session is most likely to repeat
/// verbatim.
pub const SEEDED_PROGRAM_SOURCES: &[&str] =
    &["println(\"Hello World\")", "println(1 + 1)", "1 + 1"];

/// Version of the seeded-cache format. Increment on breaking changes to
/// [`SeededCache`]'s shape (independent of the Base cache's `CACHE_VERSION`
/// in `precompile.rs` — this is a much smaller, separate artifact).
///
/// Bumped 1 -> 2 (Issue #10216 review): `source_hash` replaced the separate
/// `compiler_build_fingerprint`/`schema_fingerprint`/`enum_variant_fingerprint`
/// fields, which validated the Rust compiler/schema but never the actual
/// Base/prelude Julia SOURCE. A `src/julia/base/*.jl` edit that changed
/// Base's logic but not the Rust code or wire schema left those three
/// fingerprints unchanged, so a seeded cache built before the edit would
/// still validate and serve a stale, pre-edit `CompiledProgram` for an
/// exact-match seed like `println("Hello World")` — silently executing
/// outdated Base bytecode instead of the current bundled sources.
/// `compute_base_cache_hash()` is the same combined
/// prelude-source+schema+compiler hash the main Base cache already uses for
/// exactly this (Issues #7515/#8444); reusing it here closes the gap
/// instead of re-deriving a parallel (and previously incomplete) check.
const SEEDED_CACHE_VERSION: u32 = 2;

/// One seeded entry, with its `CompiledProgram` kept as an UNDECODED
/// postcard-encoded byte blob (see module doc comment for why).
#[derive(serde::Serialize, serde::Deserialize)]
struct SeededEntryRaw {
    /// The same hash `compile_with_cache_with_globals` computes at runtime
    /// via `compute_program_hash` for an identically-sourced one-shot CLI
    /// compile (empty `global_types`/`global_struct_names`).
    hash: u64,
    /// `precompile::cache_serialize(&CompiledProgram)`'s output. Decoded
    /// lazily, only for the entry whose hash actually matches.
    compiled_bytes: Vec<u8>,
}

/// A `CompiledProgram` embeds the same wire-format enums (`Instr`,
/// `BuiltinId`, `Intrinsic`, `BuiltinOp`) the Base cache does, so a seeded
/// cache built by a different compiler/schema/enum-layout must be rejected
/// the same way a stale Base cache is (Issues #8444/#8626) — never silently
/// misdecoded.
#[derive(serde::Serialize, serde::Deserialize)]
struct SeededCache {
    version: u32,
    /// `precompile::compute_base_cache_hash()` — combines the Base/prelude
    /// SOURCE hash with the schema and compiler-build fingerprints, so any
    /// of the three going stale invalidates a seeded cache the same way it
    /// would the main Base cache.
    source_hash: String,
    entries: Vec<SeededEntryRaw>,
}

/// Compile one seed source and return its `(hash, CompiledProgram)`, hashed
/// exactly the way a real one-shot CLI compile of the same source would
/// (`compile_with_cache`'s empty `global_types`/`global_struct_names`).
fn compile_seed(src: &str) -> Result<(u64, CompiledProgram), String> {
    // Strict soft-scope + no script path/base dir: matches how the CLI
    // compiles `sjulia file.jl` / `sjulia -e` for a program with no
    // ambiguous top-level loop assignment (the only case `script_path` would
    // otherwise affect), which every entry here is.
    let program = crate::pipeline::parse_and_lower_with_base_dir_mode(
        src,
        None,
        crate::pipeline::SoftScopeMode::Strict,
        None,
    )
    .map_err(|e| format!("Seeded program {src:?} failed to parse/lower: {e}"))?;
    let compiled = super::compile_with_cache(&program)
        .map_err(|e| format!("Seeded program {src:?} failed to compile: {e:?}"))?;
    let hash = super::cache::compute_program_hash(
        &program,
        &std::collections::HashMap::new(),
        &std::collections::HashMap::new(),
    );
    Ok((hash, compiled))
}

/// Generate the seeded program cache bytes for build-time embedding
/// (`sjulia --precompile-seeded <path>`).
pub fn generate_seeded_program_cache() -> Result<Vec<u8>, String> {
    let mut entries = Vec::with_capacity(SEEDED_PROGRAM_SOURCES.len() * 2);
    for src in SEEDED_PROGRAM_SOURCES {
        // `compute_program_hash` hashes the parsed Program (including spans),
        // and a trailing newline shifts the main block's span, so it is NOT
        // whitespace-insensitive. Seed BOTH the bare template (matches
        // `sjulia -e '<code>'`, where the shell argument rarely carries a
        // trailing newline) and the template plus a trailing newline
        // (matches a `.jl` file saved by a normal editor, which conventionally
        // ends with one) so a first-run hit isn't dependent on which
        // invocation form the user happens to use.
        for candidate in [src.to_string(), format!("{src}\n")] {
            let (hash, compiled) = compile_seed(&candidate)?;
            let compiled_bytes = super::precompile::cache_serialize(&compiled)
                .map_err(|e| format!("Seeded program {candidate:?} failed to encode: {e}"))?;
            entries.push(SeededEntryRaw {
                hash,
                compiled_bytes,
            });
        }
    }

    let cache = SeededCache {
        version: SEEDED_CACHE_VERSION,
        source_hash: super::precompile::compute_base_cache_hash(),
        entries,
    };
    super::precompile::cache_serialize(&cache)
}

fn embedded_seeded_cache_bytes() -> Option<&'static [u8]> {
    #[cfg(has_embedded_seeded_program_cache)]
    {
        Some(include_bytes!(env!("SJULIA_SEEDED_PROGRAM_CACHE_PATH")))
    }
    #[cfg(not(has_embedded_seeded_program_cache))]
    {
        None
    }
}

/// Load the embedded seeded cache's `(hash, raw CompiledProgram bytes)`
/// pairs, if present and valid, WITHOUT decoding any `CompiledProgram` (see
/// module doc comment). Never panics: any absence, decode failure, or
/// fingerprint mismatch (stale/foreign cache) degrades to an empty list, the
/// same way an invalid embedded Base cache degrades to source compilation —
/// a seeded-cache miss is exactly as cheap as if seeding were never attempted.
pub(super) fn load_embedded_seeded_entries() -> Vec<(u64, Vec<u8>)> {
    let Some(bytes) = embedded_seeded_cache_bytes() else {
        return Vec::new();
    };
    let cache: SeededCache = match super::precompile::cache_deserialize(bytes) {
        Ok(cache) => cache,
        Err(_) => return Vec::new(),
    };
    if cache.version != SEEDED_CACHE_VERSION
        || cache.source_hash != super::precompile::compute_base_cache_hash()
    {
        return Vec::new();
    }
    cache
        .entries
        .into_iter()
        .map(|entry| (entry.hash, entry.compiled_bytes))
        .collect()
}

/// Decode ONE seeded entry's `CompiledProgram` on demand (Issue #10120: the
/// whole point of keeping entries as raw bytes is to pay this decode cost
/// only for the entry that actually matches, not for every embedded seed).
pub(super) fn decode_seeded_compiled_program(bytes: &[u8]) -> Result<CompiledProgram, String> {
    super::precompile::cache_deserialize(bytes)
}

#[cfg(test)]
#[allow(clippy::unwrap_used, clippy::expect_used)]
mod tests {
    use super::*;

    /// The seeded cache is generated and consumed by the SAME process/build
    /// here (no `include_bytes!` embedding in a unit test), but the
    /// generate -> serialize -> deserialize round trip and the hash
    /// computed for each seed source should still line up with what
    /// `compile_with_cache_with_globals` computes for an identical one-shot
    /// program, since both call the same `compute_program_hash`.
    #[test]
    fn seeded_program_hashes_match_a_fresh_compile_10120() {
        for src in SEEDED_PROGRAM_SOURCES {
            let program = crate::pipeline::parse_and_lower_with_base_dir_mode(
                src,
                None,
                crate::pipeline::SoftScopeMode::Strict,
                None,
            )
            .unwrap_or_else(|e| panic!("seed {src:?} should parse/lower: {e}"));
            let hash_a = super::super::cache::compute_program_hash(
                &program,
                &std::collections::HashMap::new(),
                &std::collections::HashMap::new(),
            );
            // Re-parsing the identical source must produce the identical hash
            // (same span layout, same content) -- this is the exact invariant
            // `load_embedded_seeded_entries`'s consumer (`PROGRAM_CACHE`)
            // relies on to hit on a real run's FIRST compile of this source.
            let program_b = crate::pipeline::parse_and_lower_with_base_dir_mode(
                src,
                None,
                crate::pipeline::SoftScopeMode::Strict,
                None,
            )
            .unwrap_or_else(|e| panic!("seed {src:?} should re-parse/lower: {e}"));
            let hash_b = super::super::cache::compute_program_hash(
                &program_b,
                &std::collections::HashMap::new(),
                &std::collections::HashMap::new(),
            );
            assert_eq!(
                hash_a, hash_b,
                "seed {src:?} must hash identically across two identical parses"
            );
        }
    }

    /// `generate_seeded_program_cache` must succeed for every listed seed,
    /// round-trip through the same postcard codec the Base cache uses, and
    /// each entry's raw `compiled_bytes` must decode back to an equivalent
    /// `CompiledProgram` via `decode_seeded_compiled_program`. Each template
    /// seeds two entries (bare and trailing-newline variants, see
    /// `generate_seeded_program_cache`'s doc comment), and the resulting
    /// hashes must all be distinct (Issue #10120): a collision would mean
    /// either one entry silently overwrote another's `PROGRAM_CACHE` slot at
    /// install time, or (if it happened between a template's own bare/
    /// trailing-newline pair) that `compute_program_hash` turned out to be
    /// whitespace-insensitive after all, making the second variant redundant.
    /// This is the one test compiling all seeds (a real Base-backed compile
    /// per entry), so it deliberately covers the newline-sensitivity
    /// invariant too instead of paying that compile cost again in a second
    /// test.
    #[test]
    fn generate_seeded_program_cache_round_trips_10120() {
        let bytes = generate_seeded_program_cache().expect("seeded cache should generate");
        let cache: SeededCache =
            super::super::precompile::cache_deserialize(&bytes).expect("should decode");
        assert_eq!(cache.version, SEEDED_CACHE_VERSION);
        assert_eq!(
            cache.source_hash,
            super::super::precompile::compute_base_cache_hash(),
            "a freshly generated seeded cache must validate against the current \
             Base/prelude source"
        );
        assert_eq!(cache.entries.len(), SEEDED_PROGRAM_SOURCES.len() * 2);

        let mut hashes: Vec<u64> = cache.entries.iter().map(|e| e.hash).collect();
        hashes.sort_unstable();
        hashes.dedup();
        assert_eq!(
            hashes.len(),
            cache.entries.len(),
            "every seeded entry must have a distinct hash (including each template's \
             bare vs. trailing-newline pair)"
        );

        for entry in &cache.entries {
            let compiled = decode_seeded_compiled_program(&entry.compiled_bytes)
                .expect("each entry's raw bytes should decode to a CompiledProgram");
            assert!(
                !compiled.functions.is_empty(),
                "a seeded CompiledProgram should contain the merged Base + seed functions"
            );
        }
    }

    /// Negative test (Issue #10216 review): a seeded cache whose `source_hash`
    /// no longer matches `compute_base_cache_hash()` — the situation after a
    /// `src/julia/base/*.jl` edit changed Base's logic without touching the
    /// Rust compiler/schema — must be treated as stale and rejected, exactly
    /// like `load_embedded_seeded_entries`'s validation does for the real
    /// `include_bytes!`-embedded cache. Mirrors that function's check rather
    /// than calling it directly, since the embedded bytes there come from a
    /// compile-time `include_bytes!` path a unit test cannot swap out.
    #[test]
    fn stale_source_hash_is_rejected_10216() {
        let bytes = generate_seeded_program_cache().expect("seeded cache should generate");
        let mut cache: SeededCache =
            super::super::precompile::cache_deserialize(&bytes).expect("should decode");
        assert_eq!(
            cache.source_hash,
            super::super::precompile::compute_base_cache_hash()
        );

        cache.source_hash = "stale-hash-from-before-a-base-jl-edit".to_string();
        assert_ne!(
            cache.source_hash,
            super::super::precompile::compute_base_cache_hash(),
            "the tampered hash must actually differ from the current one for this test \
             to exercise the rejection path"
        );

        // Same condition `load_embedded_seeded_entries` checks before trusting
        // a decoded `SeededCache`.
        let is_stale = cache.version != SEEDED_CACHE_VERSION
            || cache.source_hash != super::super::precompile::compute_base_cache_hash();
        assert!(
            is_stale,
            "a SeededCache with an outdated source_hash must be rejected, not served"
        );
    }
}
