//! Pipeline logic for parsing and lowering Julia source code.
//!
//! This module handles the transformation pipeline:
//! Julia source → Parser → CST → Lowering → Core IR

use crate::error::{SyntaxError, UnsupportedFeature};
use crate::ir::core::{
    DefinitionOrderCursor, MetaAnnotation, Program, Stmt, BASE_USER_MAIN_BOUNDARY_META,
};
use crate::julia::base;
use crate::loader::{LoadError, LoaderConfig, PackageLoader};
use crate::lowering::{Lowering, LoweringWithInclude};
use crate::parser::Parser;

use once_cell::sync::Lazy;
use serde::{Deserialize, Serialize};
use sha2::Digest;
use std::env;
use std::fs;
use std::io::ErrorKind;
use std::path::{Path, PathBuf};
use std::time::{Duration, SystemTime};

/// Error variants produced by the parse-and-lower pipeline.
#[derive(Debug)]
pub enum PipelineError {
    /// Source code failed to parse.
    Parse(SyntaxError),
    /// Lowering to Core IR failed due to an unsupported feature.
    Lower(UnsupportedFeature),
    /// Loading a stdlib/package referenced by `using` failed.
    Load(LoadError),
}

impl std::fmt::Display for PipelineError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            PipelineError::Parse(e) => write!(f, "parse error: {}", e),
            PipelineError::Lower(e) => write!(f, "lowering error: {:?}", e),
            PipelineError::Load(e) => write!(f, "load error: {}", e),
        }
    }
}

impl std::error::Error for PipelineError {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        None
    }
}

/// Result of parsing and lowering Julia source code.
pub type PipelineResult = Result<Program, PipelineError>;

/// Bumped to 8 for Issue #11737: serialized modules now carry explicit
/// Base/package provenance used by source-chronology collection.
/// Version 7 for Issue #11685: lowering-generated callables now carry an
/// explicit private-helper provenance marker instead of ambiguous order zero.
/// Version 6 added complete definition chronology across retained fragments.
/// Version 5 for Issues #11036/#11128: `using`/`import` spans now carry
/// evaluation ordinals used to insert independently lowered package Modules
/// at their source position. Version 4 preserved complete explicit inner-
/// constructor self patterns (Issue #10959).
/// Version 3 added the authoritative Base/user struct provenance.
/// Version 2 was introduced by Issue #8626 for the lowered-IR enum fingerprint.
const PRELUDE_PROGRAM_CACHE_VERSION: u32 = 8;

#[derive(Debug, Serialize, Deserialize)]
struct SerializedPreludeProgram {
    version: u32,
    source_hash: String,
    /// Wire-format enum variant fingerprint (Issue #8626). See
    /// `compile::precompile::enum_variant_fingerprint`.
    enum_variant_fingerprint: String,
    program: Program,
}

struct PersistentPreludeLock {
    path: PathBuf,
}

impl Drop for PersistentPreludeLock {
    fn drop(&mut self) {
        let _ = fs::remove_file(&self.path);
    }
}

/// Cached prelude program (parsed and lowered once per process).
static PRELUDE_PROGRAM: Lazy<Option<Program>> = Lazy::new(load_prelude_program);

/// Get the Base program (used by compile_core_program)
pub fn get_prelude_program() -> Option<&'static Program> {
    PRELUDE_PROGRAM.as_ref()
}

fn compute_prelude_source_hash() -> String {
    // Include the same compiler/VM source fingerprint as the Base bytecode
    // cache. The prelude Program cache stores lowered IR, so lowering changes
    // can invalidate it even when the Julia prelude source is unchanged
    // (Issue #7544).
    static HASH: Lazy<String> = Lazy::new(|| {
        let mut hasher = sha2::Sha256::new();
        hasher.update(crate::compile::precompile::compute_prelude_hash().as_bytes());
        hasher.update(b"\0");
        hasher.update(env!("SJULIA_BASE_CACHE_BUILD_HASH").as_bytes());
        format!("{:x}", hasher.finalize())
    });
    HASH.clone()
}

fn persistent_prelude_cache_disabled() -> bool {
    env::var("SUBSET_JULIA_VM_DISABLE_PERSISTENT_PRELUDE_CACHE").is_ok()
}

fn workspace_target_dir() -> PathBuf {
    if let Ok(target_dir) = env::var("CARGO_TARGET_DIR") {
        return PathBuf::from(target_dir);
    }

    Path::new(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .map(|p| p.join("target"))
        .unwrap_or_else(|| PathBuf::from("target"))
}

fn persistent_prelude_cache_path() -> PathBuf {
    let hash = compute_prelude_source_hash();
    workspace_target_dir().join(format!("sjulia_prelude_program_{hash}.bin"))
}

#[derive(Debug, Clone)]
pub struct PreludeCacheArtifactDebugStatus {
    pub state: &'static str,
    pub path: Option<PathBuf>,
    pub detail: Option<String>,
}

#[derive(Debug, Clone)]
pub struct PreludeProgramCacheFingerprints {
    pub version: u32,
    pub source_hash: String,
    pub compiler_build_fingerprint: String,
    pub enum_variant_fingerprint: String,
}

#[derive(Debug, Clone)]
pub struct PreludeProgramCacheDebugStatus {
    pub load_source: &'static str,
    pub persistent_disabled: bool,
    pub embedded: PreludeCacheArtifactDebugStatus,
    pub persistent: PreludeCacheArtifactDebugStatus,
    pub fingerprints: PreludeProgramCacheFingerprints,
}

fn validate_prelude_cache_artifact(
    path: Option<PathBuf>,
    bytes: Option<&[u8]>,
) -> PreludeCacheArtifactDebugStatus {
    let Some(bytes) = bytes else {
        return PreludeCacheArtifactDebugStatus {
            state: "missing",
            path,
            detail: None,
        };
    };

    match deserialize_prelude_program(bytes) {
        Ok(_) => PreludeCacheArtifactDebugStatus {
            state: "valid",
            path,
            detail: None,
        },
        Err(e) => PreludeCacheArtifactDebugStatus {
            state: "invalid",
            path,
            detail: Some(e),
        },
    }
}

fn read_prelude_cache_artifact_without_side_effects(
    path: &Path,
) -> PreludeCacheArtifactDebugStatus {
    match fs::read(path) {
        Ok(bytes) => validate_prelude_cache_artifact(Some(path.to_path_buf()), Some(&bytes)),
        Err(e) if e.kind() == ErrorKind::NotFound => PreludeCacheArtifactDebugStatus {
            state: "missing",
            path: Some(path.to_path_buf()),
            detail: None,
        },
        Err(e) => PreludeCacheArtifactDebugStatus {
            state: "invalid",
            path: Some(path.to_path_buf()),
            detail: Some(format!("read failed: {e}")),
        },
    }
}

/// Report how the parsed/lowered prelude Program cache would load without
/// removing stale persistent files or reparsing source (Issue #8718).
pub fn prelude_program_cache_debug_status() -> PreludeProgramCacheDebugStatus {
    let persistent_disabled = persistent_prelude_cache_disabled();
    let embedded = validate_prelude_cache_artifact(None, embedded_prelude_program_bytes());
    let persistent_path = persistent_prelude_cache_path();
    let persistent = if persistent_disabled {
        PreludeCacheArtifactDebugStatus {
            state: "disabled",
            path: Some(persistent_path),
            detail: None,
        }
    } else {
        read_prelude_cache_artifact_without_side_effects(&persistent_path)
    };
    let load_source = if persistent_disabled {
        "none"
    } else if embedded.state == "valid" {
        "embedded"
    } else if persistent.state == "valid" {
        "persistent"
    } else {
        "none"
    };

    PreludeProgramCacheDebugStatus {
        load_source,
        persistent_disabled,
        embedded,
        persistent,
        fingerprints: PreludeProgramCacheFingerprints {
            version: PRELUDE_PROGRAM_CACHE_VERSION,
            source_hash: compute_prelude_source_hash(),
            compiler_build_fingerprint: crate::compile::precompile::compiler_build_fingerprint()
                .to_string(),
            enum_variant_fingerprint: crate::compile::precompile::enum_variant_fingerprint(),
        },
    }
}

fn parse_prelude_from_source() -> Option<Program> {
    crate::compile::profile::cold_reset();
    let result = parse_prelude_from_source_batched();
    crate::compile::profile::cold_print_summary("parse_prelude_from_source");
    result
}

/// Parse and lower the Base prelude file-by-file (Issue #10119 / #10122).
///
/// Historically this parsed+lowered `base::get_base()`'s single concatenated
/// string in ONE `Parser::parse` + ONE `Lowering::lower` call. That hid the
/// per-file cost breakdown and made the ~65-file parse fully sequential
/// despite the files having no shared PARSE-time state.
///
/// This splits the pipeline into two phases:
/// 1. **Parse** every file independently (`parse_all_base_files`) — pure CST
///    construction, no shared lowering state, safe to run in parallel.
/// 2. **Lower** every file's CST sequentially, IN FILE ORDER, sharing ONE
///    [`crate::lowering::LambdaContext`] and ONE type-alias scope across the
///    whole batch via
///    [`crate::lowering::LoweringWithInclude::lower_fragment_with_shared_context`].
///    This preserves the exact semantics of lowering the historical
///    concatenated whole text in a single pass — a type alias or a lifted
///    anonymous-lambda name from file N stays visible/non-colliding while
///    lowering file N+1 — just performed incrementally instead of over one
///    giant string. Lowering itself is NOT parallelized: `macros.jl` (file
///    38) defines macros later files expand at lowering time, so lowering
///    order must match file order.
///
/// Verified byte-for-byte equivalent (function/struct/module counts and a
/// representative fixture run) against the historical whole-text path by
/// `prelude_batched_lowering_matches_whole_text_lowering` in the test module
/// below.
fn parse_prelude_from_source_batched() -> Option<Program> {
    use crate::compile::profile;
    use crate::lowering::{type_alias, IncludeContext, LambdaContext, LoweringWithInclude};

    let files = base::BASE_FILE_SOURCES;
    let parsed = parse_all_base_files(files);

    let lambda_ctx = LambdaContext::new();
    let alias_scope = type_alias::snapshot();

    let mut program = empty_program();

    for ((name, src), outcome) in files.iter().zip(parsed) {
        let outcome = outcome.ok()?;
        let mut lowering = LoweringWithInclude::new_with_file(
            src,
            IncludeContext::new(None),
            Some(PathBuf::from(*name)),
        );
        let label: String = if profile::cold_enabled() {
            format!("lower.{name}")
        } else {
            String::new()
        };
        let fragment = profile::cold_time_immediate(label, || {
            lowering.lower_fragment_with_shared_context(outcome, &lambda_ctx)
        })
        .ok()?;
        merge_program_fragment_into(&mut program, fragment);
    }

    alias_scope.restore();
    program.mark_structs_as_base_origin();
    Some(program)
}

/// An empty [`Program`] accumulator for [`parse_prelude_from_source_batched`].
fn empty_program() -> Program {
    Program {
        abstract_types: Vec::new(),
        primitive_types: Vec::new(),
        type_aliases: Vec::new(),
        structs: Vec::new(),
        functions: Vec::new(),
        base_function_count: 0,
        modules: Vec::new(),
        usings: Vec::new(),
        macros: Vec::new(),
        enums: Vec::new(),
        main: crate::ir::core::Block {
            stmts: Vec::new(),
            span: crate::span::Span::new(0, 0, 0, 0, 0, 0),
        },
    }
}

/// Merge one per-file lowered fragment into the accumulated prelude
/// [`Program`], mirroring [`crate::lowering::IncludedContent::merge_into`]'s
/// field list (each fragment is already fully self-contained — macro- and
/// kwdef-expanded structs/macros are folded in during that file's own
/// lowering pass, same as an `include()`d file).
///
/// Issue #10164: `LoweringWithInclude` (used per-file below, to share one
/// `LambdaContext` across the batch) correctly captures a top-level docstring
/// into a `__sjulia_doc_<Name>` main-block `Assign` ahead of the following
/// definition. This used to be filtered back out here to stay behaviorally
/// identical to the historical whole-text `Lowering::lower` path, which never
/// populated `pending_doc` and so never registered Base docstrings. Now that
/// `Lowering::lower_source_file_inner` captures docstrings the same way (see
/// `lowering::mod::Lowering::lower_source_file_inner`), both lowering paths
/// agree and Base's own docstrings (`Val`, `Exception`, `BoundsError`, etc.)
/// are registered like every other definition.
fn merge_program_fragment_into(accum: &mut Program, fragment: Program) {
    accum.abstract_types.extend(fragment.abstract_types);
    accum.primitive_types.extend(fragment.primitive_types);
    accum.type_aliases.extend(fragment.type_aliases);
    accum.structs.extend(fragment.structs);
    accum.functions.extend(fragment.functions);
    accum.modules.extend(fragment.modules);
    accum.usings.extend(fragment.usings);
    accum.macros.extend(fragment.macros);
    accum.enums.extend(fragment.enums);
    accum.main.stmts.extend(fragment.main.stmts);
}

/// Parse every Base file. Pure CST construction (no shared lowering state),
/// so non-wasm targets run every file's parse on its own thread
/// (`std::thread::scope`, Issue #10122) instead of the historical fully
/// sequential single-string parse. WASM (no threads) falls back to
/// sequential parsing of each file.
#[cfg(not(target_arch = "wasm32"))]
fn parse_all_base_files(
    files: &'static [(&'static str, &'static str)],
) -> Vec<Result<crate::parser::ParseOutcome, SyntaxError>> {
    use crate::compile::profile;
    use std::time::Instant;

    profile::cold_note_immediate(
        "parse phase ran on one thread per file; durations below are per-thread \
         CPU time, not exclusive wall time (files overlap in real time)",
    );

    std::thread::scope(|scope| {
        let handles: Vec<_> = files
            .iter()
            .map(|(name, src)| {
                scope.spawn(move || {
                    let start = Instant::now();
                    let result = parse_one_base_file(src);
                    (*name, start.elapsed(), result)
                })
            })
            .collect();

        handles
            .into_iter()
            .map(|handle| {
                let (name, elapsed, result) = handle.join().unwrap_or_else(|_| {
                    (
                        "<panicked>",
                        std::time::Duration::ZERO,
                        Err(SyntaxError::parse_failed(
                            "Base file parser thread panicked".to_string(),
                        )),
                    )
                });
                if profile::cold_enabled() {
                    profile::cold_record_immediate(format!("parse.{name}"), elapsed);
                }
                result
            })
            .collect()
    })
}

/// WASM has no threads: parse every Base file sequentially (same per-file
/// timing instrumentation as the parallel path).
#[cfg(target_arch = "wasm32")]
fn parse_all_base_files(
    files: &'static [(&'static str, &'static str)],
) -> Vec<Result<crate::parser::ParseOutcome, SyntaxError>> {
    use crate::compile::profile;
    use std::time::Instant;

    files
        .iter()
        .map(|(name, src)| {
            let start = Instant::now();
            let result = parse_one_base_file(src);
            if profile::cold_enabled() {
                profile::cold_record_immediate(format!("parse.{name}"), start.elapsed());
            }
            result
        })
        .collect()
}

fn parse_one_base_file(src: &str) -> Result<crate::parser::ParseOutcome, SyntaxError> {
    let mut parser = Parser::new()
        .map_err(|e| SyntaxError::parse_failed(format!("Parser initialization failed: {}", e)))?;
    parser.parse(src)
}

fn serialize_prelude_program(program: &Program) -> Result<Vec<u8>, String> {
    let cache = SerializedPreludeProgram {
        version: PRELUDE_PROGRAM_CACHE_VERSION,
        source_hash: compute_prelude_source_hash(),
        enum_variant_fingerprint: crate::compile::precompile::enum_variant_fingerprint(),
        program: program.clone(),
    };
    bincode::serialize(&cache).map_err(|e| format!("Prelude serialization failed: {}", e))
}

fn deserialize_prelude_program(bytes: &[u8]) -> Result<Program, String> {
    let cache: SerializedPreludeProgram = bincode::deserialize(bytes)
        .map_err(|e| format!("Prelude deserialization failed: {}", e))?;

    if cache.version != PRELUDE_PROGRAM_CACHE_VERSION {
        return Err(format!(
            "Prelude cache version mismatch: expected {}, got {}",
            PRELUDE_PROGRAM_CACHE_VERSION, cache.version
        ));
    }
    if cache.source_hash != compute_prelude_source_hash() {
        return Err("Prelude source hash mismatch".to_string());
    }
    // Enum variant fingerprint gate (Issue #8626): the lowered Program embeds
    // wire-format enums (`BuiltinOp` in `Expr::Builtin`), so a cache built
    // under a different variant declaration order must be regenerated. The
    // embedded prelude cache (`SJULIA_PRELUDE_PROGRAM_CACHE`) is generated by
    // a binary built from the same source tree, so it matches by construction.
    if cache.enum_variant_fingerprint != crate::compile::precompile::enum_variant_fingerprint() {
        return Err("Prelude cache enum variant fingerprint mismatch".to_string());
    }

    Ok(cache.program)
}

fn embedded_prelude_program_bytes() -> Option<&'static [u8]> {
    #[cfg(has_embedded_prelude_program)]
    {
        Some(include_bytes!(env!("SJULIA_PRELUDE_PROGRAM_CACHE_PATH")))
    }
    #[cfg(not(has_embedded_prelude_program))]
    {
        None
    }
}

fn load_embedded_prelude_program() -> Option<Program> {
    let bytes = embedded_prelude_program_bytes()?;
    deserialize_prelude_program(bytes).ok()
}

fn read_persistent_prelude_cache(path: &Path) -> Option<Program> {
    let bytes = match fs::read(path) {
        Ok(bytes) => bytes,
        Err(e) if e.kind() == ErrorKind::NotFound => return None,
        Err(_) => return None,
    };

    match deserialize_prelude_program(&bytes) {
        Ok(program) => Some(program),
        Err(_) => {
            let _ = fs::remove_file(path);
            None
        }
    }
}

fn acquire_persistent_prelude_lock(cache_path: &Path) -> Option<PersistentPreludeLock> {
    let lock_path = cache_path.with_extension("lock");
    let stale_after = Duration::from_secs(20 * 60);

    for _ in 0..1200 {
        match fs::OpenOptions::new()
            .write(true)
            .create_new(true)
            .open(&lock_path)
        {
            Ok(_) => {
                return Some(PersistentPreludeLock { path: lock_path });
            }
            Err(e) if e.kind() == ErrorKind::AlreadyExists => {
                if let Ok(metadata) = fs::metadata(&lock_path) {
                    let is_stale = metadata
                        .modified()
                        .ok()
                        .and_then(|modified| SystemTime::now().duration_since(modified).ok())
                        .is_some_and(|age| age > stale_after);
                    if is_stale {
                        let _ = fs::remove_file(&lock_path);
                    }
                }
                if cache_path.exists() {
                    return None;
                }
                std::thread::sleep(Duration::from_millis(50));
            }
            Err(_) => return None,
        }
    }

    None
}

fn write_persistent_prelude_cache(path: &Path, program: &Program) {
    let Some(parent) = path.parent() else {
        return;
    };
    if fs::create_dir_all(parent).is_err() {
        return;
    }

    let Ok(bytes) = serialize_prelude_program(program) else {
        return;
    };

    let tmp_path = path.with_extension(format!("tmp.{}", std::process::id()));
    if fs::write(&tmp_path, bytes).is_err() {
        return;
    }
    if fs::rename(&tmp_path, path).is_err() {
        let _ = fs::remove_file(&tmp_path);
    }
}

fn load_prelude_program() -> Option<Program> {
    if persistent_prelude_cache_disabled() {
        return parse_prelude_from_source();
    }

    if let Some(program) = load_embedded_prelude_program() {
        return Some(program);
    }

    let cache_path = persistent_prelude_cache_path();
    if let Some(program) = read_persistent_prelude_cache(&cache_path) {
        return Some(program);
    }

    let Some(_lock) = acquire_persistent_prelude_lock(&cache_path) else {
        return read_persistent_prelude_cache(&cache_path).or_else(parse_prelude_from_source);
    };

    if let Some(program) = read_persistent_prelude_cache(&cache_path) {
        return Some(program);
    }

    let program = parse_prelude_from_source()?;
    write_persistent_prelude_cache(&cache_path, &program);
    Some(program)
}

/// Generate and serialize the parsed/lowered prelude Program for build-time embedding.
pub fn generate_prelude_program_cache() -> Result<Vec<u8>, String> {
    let program = parse_prelude_from_source().ok_or("Failed to parse and lower prelude")?;
    serialize_prelude_program(&program)
}

/// Parse source code without prelude merging (used for prelude itself)
pub fn parse_source(src: &str) -> PipelineResult {
    let mut parser = Parser::new().map_err(|e| {
        PipelineError::Parse(SyntaxError::parse_failed(format!(
            "Parser initialization failed: {}",
            e
        )))
    })?;

    let outcome = parser.parse(src).map_err(PipelineError::Parse)?;

    // Macro expansion seam (Issue #8656): idempotent install of the VM-backed expander.
    crate::macro_runtime::install();
    let mut lowering = Lowering::new(src);
    lowering.lower(outcome).map_err(PipelineError::Lower)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn prelude_program_cache_roundtrips() {
        let bytes = generate_prelude_program_cache().expect("generate prelude cache");
        let program = deserialize_prelude_program(&bytes).expect("deserialize prelude cache");
        assert!(
            !program.functions.is_empty(),
            "prelude cache should contain lowered Base functions"
        );
        assert_eq!(
            program.base_function_count, 0,
            "raw prelude Program is not merged into a user Program yet"
        );
    }

    /// Issue #10119/#10122: the new per-file batched parse+lower
    /// (`parse_prelude_from_source_batched`) must produce a prelude
    /// structurally equivalent to the historical single whole-text parse+lower
    /// (`parse_source(&base::get_base())`). Spans legitimately differ (each
    /// fragment's spans are byte offsets into ITS OWN file text, not the old
    /// concatenated whole-text buffer), so this compares content, not spans:
    /// the same set of function signatures, struct/abstract-type/module names,
    /// and main-block statement count.
    #[test]
    fn prelude_batched_lowering_matches_whole_text_lowering_10119() {
        fn function_signature(f: &crate::ir::core::Function) -> String {
            let params: Vec<String> = f
                .params
                .iter()
                .map(|p| p.effective_type().to_string())
                .collect();
            format!("{}({})", f.name, params.join(", "))
        }

        let whole_text = base::get_base();
        let old_program = parse_source(&whole_text).expect("whole-text parse+lower");
        let new_program =
            parse_prelude_from_source_batched().expect("batched per-file parse+lower");

        let mut old_sigs: Vec<String> = old_program
            .functions
            .iter()
            .map(|f| function_signature(f))
            .collect();
        let mut new_sigs: Vec<String> = new_program
            .functions
            .iter()
            .map(|f| function_signature(f))
            .collect();
        old_sigs.sort();
        new_sigs.sort();
        assert_eq!(
            old_sigs, new_sigs,
            "batched per-file lowering must define the exact same set of function signatures \
             as the historical whole-text lowering (a mismatch here would mean either a lost \
             method or a spurious duplicate — e.g. from a lifted-lambda name collision across \
             files, Issue #10122)"
        );

        let mut old_structs: Vec<&str> = old_program
            .structs
            .iter()
            .map(|s| s.name.as_str())
            .collect();
        let mut new_structs: Vec<&str> = new_program
            .structs
            .iter()
            .map(|s| s.name.as_str())
            .collect();
        old_structs.sort();
        new_structs.sort();
        assert_eq!(old_structs, new_structs, "struct set must match");

        let mut old_abstract: Vec<&str> = old_program
            .abstract_types
            .iter()
            .map(|a| a.name.as_str())
            .collect();
        let mut new_abstract: Vec<&str> = new_program
            .abstract_types
            .iter()
            .map(|a| a.name.as_str())
            .collect();
        old_abstract.sort();
        new_abstract.sort();
        assert_eq!(old_abstract, new_abstract, "abstract type set must match");

        let mut old_modules: Vec<&str> = old_program
            .modules
            .iter()
            .map(|m| m.name.as_str())
            .collect();
        let mut new_modules: Vec<&str> = new_program
            .modules
            .iter()
            .map(|m| m.name.as_str())
            .collect();
        old_modules.sort();
        new_modules.sort();
        assert_eq!(old_modules, new_modules, "module set must match");

        assert_eq!(
            old_program.type_aliases.len(),
            new_program.type_aliases.len(),
            "type alias count must match"
        );
        assert_eq!(
            old_program.main.stmts.len(),
            new_program.main.stmts.len(),
            "main-block statement count must match"
        );
    }

    /// Issue #8626: a prelude Program cache built under a different enum
    /// variant declaration order must be rejected cleanly (the load path then
    /// removes the stale file and re-parses from source), never misdecoded.
    #[test]
    fn prelude_cache_enum_fingerprint_mismatch_rejected_8626() {
        let program = parse_source("1 + 2").expect("parse trivial program");
        let stale = SerializedPreludeProgram {
            version: PRELUDE_PROGRAM_CACHE_VERSION,
            source_hash: compute_prelude_source_hash(),
            enum_variant_fingerprint:
                "0000000000000000000000000000000000000000000000000000000000000000".to_string(),
            program,
        };
        let bytes = bincode::serialize(&stale).expect("serialize stale prelude cache");
        let err = deserialize_prelude_program(&bytes)
            .expect_err("mismatched enum variant fingerprint must reject the prelude cache");
        assert!(
            err.contains("enum variant fingerprint mismatch"),
            "expected enum-variant-fingerprint rejection, got: {}",
            err
        );
    }
}

/// Soft-scope leniency for top-level `for`/`while` bodies (Issue #9210).
///
/// Upstream Julia distinguishes the interactive REPL (lenient soft scope) from
/// non-interactive script execution (strict soft scope). See
/// [`crate::lowering::soft_scope`] for the full semantics.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum SoftScopeMode {
    /// REPL-style leniency: a top-level loop assignment to an existing global
    /// mutates it (Issues #8691 / #8715). Used by the interactive REPL and by the
    /// internal lenient entry points (`parse_and_lower` /
    /// `parse_and_lower_with_base_dir`: prelude/Base cache compilation, the
    /// fixture harness).
    Lenient,
    /// File/module (non-interactive script) strictness: a top-level loop
    /// assignment to an existing global binds a NEW local, so a read-before-write
    /// (`+=`) raises `UndefVarError`, matching `julia file.jl` / `julia -e`. Used
    /// by the CLI, the C ABI editor entries, and the WASM `run_from_source` host
    /// (Issue #9283).
    Strict,
}

/// Parse and lower Julia source code using the pure-Rust `subset_julia_vm_parser`
/// and the unified `Lowering` pipeline.
/// Merges prelude functions and structs with user code.
pub fn parse_and_lower(src: &str) -> PipelineResult {
    parse_and_lower_with_base_dir(src, None)
}

/// Parse and lower Julia source code with include support.
/// The base_dir is used to resolve relative paths in include() calls.
///
/// Uses [`SoftScopeMode::Lenient`]. This wrapper stays lenient because it is
/// used by internal callers (prelude/Base cache compilation, the fixture test
/// harness). Non-interactive script/host surfaces opt into strict soft scope via
/// [`parse_and_lower_with_base_dir_mode`] / [`parse_and_lower_strict`].
pub fn parse_and_lower_with_base_dir(src: &str, base_dir: Option<PathBuf>) -> PipelineResult {
    parse_and_lower_with_base_dir_mode(src, base_dir, SoftScopeMode::Lenient, None)
}

/// Parse and lower a whole-program buffer under **strict file-mode soft scope**
/// (Issue #9210), the editor/"Run" behaviour the C ABI and WASM hosts adopt so a
/// buffer matches `julia file.jl` (Issue #9283). No script path is threaded, so
/// the soft-scope warning locates at `none:<line>` — matching upstream for an
/// eval'd buffer with no backing file. The interactive REPL (`REPLSession`)
/// keeps lenient soft scope and never routes through here.
pub fn parse_and_lower_strict(src: &str) -> PipelineResult {
    parse_and_lower_with_base_dir_mode(src, None, SoftScopeMode::Strict, None)
}

/// Like [`parse_and_lower_with_base_dir`] but with an explicit [`SoftScopeMode`]
/// and an optional `script_path` for the soft-scope warning location.
///
/// The non-interactive CLI (`sjulia file.jl` / `-e` / piped stdin) and the C ABI
/// / WASM hosts pass [`SoftScopeMode::Strict`] so top-level loop-body assignments
/// to existing globals bind new locals (Issue #9210), matching `julia file.jl`.
/// `script_path` is the absolute path of the backing file (for `sjulia file.jl`),
/// used only to render the warning location (`└ @ /abs/path:<line>`); pass `None`
/// for `-e` / stdin / host buffers, where upstream prints `└ @ none:<line>`
/// (Issue #9283). The interactive REPL never routes through here (it lowers via
/// `Lowering`).
pub fn parse_and_lower_with_base_dir_mode(
    src: &str,
    base_dir: Option<PathBuf>,
    soft_scope_mode: SoftScopeMode,
    script_path: Option<&str>,
) -> PipelineResult {
    use crate::compile::profile;

    // Parse user source code with include support
    let mut user_program = profile::time_immediate("pipeline.parse_user", || {
        parse_source_with_include(src, base_dir)
    })?;

    // Hard-scope `let` localization (Issue #9284). A `let` is a hard local scope
    // in EVERY execution mode (REPL and `julia file.jl` both error on a
    // read-before-write of a loop-captured global inside a `let`), so this runs
    // unconditionally — it is not gated on `soft_scope_mode`. Run it before the
    // strict soft-scope pass and before the prelude merge, on the freshly parsed
    // user `main` block, so only user-authored `let`s are examined.
    crate::lowering::soft_scope::apply_hard_scope_let_localization(&mut user_program.main);

    // File/module strict soft-scope resolution (Issue #9210). Run it on the
    // freshly parsed user `main` block, before merging the prelude, so only
    // user-authored top-level loops are examined.
    if soft_scope_mode == SoftScopeMode::Strict {
        crate::lowering::soft_scope::apply_file_mode_soft_scope(
            &mut user_program.main,
            script_path,
        );
    }

    // Force the prelude Lazy first so the merge timing below reflects only the
    // per-run merge cost, not the one-time prelude deserialize (Issue #6348).
    let prelude_ref =
        profile::time_immediate("pipeline.prelude_program_load", || PRELUDE_PROGRAM.as_ref());

    // Merge prelude program (structs first, then functions)
    if let Some(prelude) = prelude_ref {
        profile::time_immediate("pipeline.merge_prelude", || {
            merge_prelude_into_user_program(prelude, &mut user_program);
        });
    }

    // Load stdlib/packages referenced by `using` statements that are not defined
    // inline. On desktop this hits the persistent `.ji.json` loader cache; on
    // iOS/WASM the cache is disabled, so every run re-parses and re-lowers the
    // bundled `.jl` sources here — the dominant per-`using` cost this profile
    // point isolates (Issue #9189).
    let mut package_loader = PackageLoader::new(LoaderConfig::from_env());
    profile::time_immediate("pipeline.load_packages", || {
        package_loader.load_into_program(&mut user_program)
    })
    .map_err(PipelineError::Load)?;

    Ok(user_program)
}

/// Merge the (process-wide cached) prelude `Program` into a freshly lowered
/// user `Program`: structs, functions (exact-signature replacement, Issue
/// #2719), abstract types, and main-block statements with the Base/user
/// boundary meta marker.
fn merge_prelude_into_user_program(prelude: &Program, user_program: &mut Program) {
    // Prelude and user source are lowered in separate contexts, so both start
    // their definition ordinals at one. Rebase the user segment after the
    // prelude before combining their independently partitioned function and
    // struct vectors (Issue #11028).
    let mut chronology = DefinitionOrderCursor::after_program(prelude);
    chronology.append_fragment(&mut *user_program);

    // Helper function to get method signature (name + parameter types)
    // This allows multiple dispatch - same name with different parameter types
    fn get_method_signature(func: &crate::ir::core::Function) -> String {
        let param_types: Vec<String> = func
            .params
            .iter()
            .map(|p| p.effective_type().to_string())
            .collect();
        format!("{}({})", func.name, param_types.join(", "))
    }

    // Collect user method signatures to avoid conflicts (for Base extensions)
    let user_method_sigs: std::collections::HashSet<_> = user_program
        .functions
        .iter()
        .map(|f| get_method_signature(f))
        .collect();

    // User-defined function NAMES only (Issue #10121). Computing
    // `get_method_signature` (formats every parameter's type into a fresh
    // String) for every one of the ~5000 prelude functions on EVERY compile
    // was the actual dominant cost of this merge — NOT the `Arc::clone`
    // refcount bump the signature filter below guards (`Program.functions` is
    // Arc-wrapped, Issue #9140, so cloning the retained functions is cheap
    // regardless). A prelude function whose NAME doesn't match ANY
    // user-defined function name cannot possibly collide on signature (a
    // signature always starts with the function name), so this small
    // `HashSet<&str>` — built once, proportional to the USER program's
    // function count, not the prelude's — lets the filter below skip the
    // expensive signature `format!`/`String` allocation entirely for the
    // overwhelming majority of prelude functions (typically ALL of them,
    // since most programs never redefine a Base function name).
    let user_function_names: std::collections::HashSet<&str> = user_program
        .functions
        .iter()
        .map(|f| f.name.as_str())
        .collect();

    // Collect user struct names to avoid conflicts
    let user_struct_names: std::collections::HashSet<String> = user_program
        .structs
        .iter()
        .map(|s| s.name.clone())
        .collect();

    // Merge structs (prelude first, but skip if user defines same name)
    let mut all_structs: Vec<crate::ir::core::StructDef> = prelude
        .structs
        .iter()
        .filter(|s| !user_struct_names.contains(s.name.as_str()))
        .cloned()
        .collect();
    all_structs.extend(std::mem::take(&mut user_program.structs));
    user_program.structs = all_structs;

    // Merge functions:
    // Filter by exact signature to support multiple dispatch (Issue #2719).
    // User-defined methods only replace base methods with the SAME signature,
    // preserving all other overloads. This matches Julia semantics where
    // defining a new method adds to (or replaces an exact match in) the
    // method table, never removing unrelated overloads.
    //
    // `Function` is Arc-wrapped in `Program.functions` (Issue #9140) so this
    // `.cloned()` bumps ~5000 prelude Arc refcounts instead of deep-cloning
    // each function's IR body — the dominant cold-start cost this merge used
    // to pay on every process/REPL-eval, regardless of user program size.
    //
    // Issue #10121: the name check short-circuits the (formerly unconditional)
    // signature computation for every prelude function that no user function
    // shadows by name — i.e. almost always all ~5000 of them.
    let mut all_functions: Vec<std::sync::Arc<crate::ir::core::Function>> = prelude
        .functions
        .iter()
        .filter(|f| {
            !user_function_names.contains(f.name.as_str())
                || !user_method_sigs.contains(&get_method_signature(f))
        })
        .cloned()
        .collect();
    // Track base function count BEFORE adding user functions
    let base_function_count = all_functions.len();
    all_functions.extend(std::mem::take(&mut user_program.functions));
    user_program.functions = all_functions;
    user_program.base_function_count = base_function_count;

    // Merge abstract types (prelude first, skip if user defines same name)
    let user_abstract_type_names: std::collections::HashSet<String> = user_program
        .abstract_types
        .iter()
        .map(|a| a.name.clone())
        .collect();
    let mut all_abstract_types: Vec<crate::ir::core::AbstractTypeDef> = prelude
        .abstract_types
        .iter()
        .filter(|a| !user_abstract_type_names.contains(a.name.as_str()))
        .cloned()
        .collect();
    all_abstract_types.extend(std::mem::take(&mut user_program.abstract_types));
    user_program.abstract_types = all_abstract_types;

    // Merge Base/prelude modules before user modules so bundled Base submodules
    // such as `Base.Order` participate in module metadata and initialization.
    let mut all_modules: Vec<crate::ir::core::Module> = prelude.modules.clone();
    all_modules.extend(std::mem::take(&mut user_program.modules));
    user_program.modules = all_modules;

    // Merge main blocks: prelude main block first (defines globals like `im`, const arrays, etc.)
    // then user program main block follows.
    // This ensures prelude const definitions are available to all functions.
    let mut merged_main_stmts = prelude.main.stmts.clone();
    merged_main_stmts.push(Stmt::Meta {
        annotation: MetaAnnotation {
            name: BASE_USER_MAIN_BOUNDARY_META.to_string(),
            args: Vec::new(),
        },
        span: crate::span::Span::new(0, 0, 0, 0, 0, 0),
    });
    merged_main_stmts.extend(std::mem::take(&mut user_program.main.stmts));
    user_program.main = crate::ir::core::Block {
        stmts: merged_main_stmts,
        span: user_program.main.span,
    };
}

/// Parse source code with include support.
pub fn parse_source_with_include(src: &str, base_dir: Option<PathBuf>) -> PipelineResult {
    let mut parser = Parser::new().map_err(|e| {
        PipelineError::Parse(SyntaxError::parse_failed(format!(
            "Parser initialization failed: {}",
            e
        )))
    })?;

    let outcome = parser.parse(src).map_err(PipelineError::Parse)?;

    // Macro expansion seam (Issue #8656): idempotent install of the VM-backed expander.
    crate::macro_runtime::install();
    let mut lowering = LoweringWithInclude::with_base_dir(src, base_dir);
    lowering.lower(outcome).map_err(PipelineError::Lower)
}
