//! Persisted VM bytecode file format for SubsetJuliaVM.
//!
//! `.sjvmbc` stores a compiled VM `CompiledProgram` for direct interpreter
//! execution. The original Core IR `Program` is stored next to the compiled
//! payload so runtime specialization context can be reconstructed after
//! deserialization.
//!
//! # Cache invalidation (Issue #10170)
//!
//! The bincode payload is positional, not self-describing: deserializing a
//! `.sjvmbc` produced by a different compiler build can silently misdecode
//! instead of failing cleanly. The header therefore embeds the same three
//! fingerprints as the Base bytecode cache (`compile::precompile`):
//!
//! - [`base_cache_schema_fingerprint`] — hash of the wire-format source files
//!   listed in `base_cache_schema_files.txt` (Instr, method-table structs,
//!   VM type metadata, ...),
//! - [`compiler_build_fingerprint`] — hash of every Rust source file in
//!   `subset_julia_vm/src` AND in the sibling crates whose serde-derived
//!   types appear in the serialized payload (`subset_julia_vm_ir`,
//!   `subset_julia_vm_types`, `subset_julia_vm_bytecode`; Issue #10332):
//!   `Program` embeds `Expr`/`JuliaType`/`TypeExpr`/`TypeParam`/`Span` from
//!   those crates, which are neither in the schema manifest nor covered by
//!   the enum-variant fingerprint, so a shape change there must invalidate
//!   through this fingerprint (see
//!   `compiler_build_fingerprint_covers_payload_dependency_crates_10332`),
//! - [`enum_variant_fingerprint`] — hash of the variant-name lists of the
//!   positional wire enums (`Instr`/`BuiltinId`/`Intrinsic`/`BuiltinOp`,
//!   Issue #8626).
//!
//! [`load`] (path) and [`load_from_bytes`] (in-memory, for host bindings that
//! read the `.sjvmbc` as a bundle resource — the iOS bundled-sample loader,
//! #10171) share the same [`load_from_reader`] validation path: both require an
//! exact [`VERSION`] match (an *older* file is just as stale as a newer one)
//! and matching fingerprints. Mismatches surface as
//! [`VmBytecodeFileError::VersionMismatch`] /
//! [`VmBytecodeFileError::FingerprintMismatch`]; callers that treat `.sjvmbc`
//! as a cache should check [`VmBytecodeFileError::is_stale_cache`] and fall
//! back to compiling the `.jl` source (the FFI loader collapses the whole
//! error class onto its stale-cache status, since the bundled source is always
//! available), while the `--run-vm-bytecode` CLI path reports the error loudly.
//!
//! [`base_cache_schema_fingerprint`]: crate::compile::precompile::base_cache_schema_fingerprint
//! [`compiler_build_fingerprint`]: crate::compile::precompile::compiler_build_fingerprint
//! [`enum_variant_fingerprint`]: crate::compile::precompile::enum_variant_fingerprint

// Issue #10906 (Phase 1c of #10869): the `.sjvmbc` cache-load boundary —
// zero real unwrap_used/expect_used sites in production code (every match is
// inside the cfg(test) module, which carries an explicit allow).
#![deny(clippy::unwrap_used)]
#![deny(clippy::expect_used)]

use crate::ir::core::Program;
use std::fs::File;
use std::io::{Read, Write};
use std::path::Path;
use subset_julia_vm_bytecode::CompiledProgram;

/// Magic bytes identifying a SubsetJuliaVM VM bytecode file.
pub const MAGIC: &[u8; 4] = b"SJVM";

/// Current persisted VM bytecode file format version.
///
/// Version history:
/// - 3: `MAGIC + version + flags + payload length + bincode payload`.
/// - 4 (Issue #10170): three length-prefixed fingerprint strings inserted
///   between the flags and the payload length; `load` rejects any
///   `version != VERSION` instead of accepting older versions.
/// - 5 (Issue #10333): `CompiledProgram` persists the inference-global type
///   snapshot used to rebuild runtime reflection state after restore.
/// - 6 (Issue #10334): `CompiledProgram` persists finalized specialization-
///   disable flags so restore does not re-derive dispatch safety from source.
/// - 7 (Issue #10339): the payload carries the promotion-registry rules the
///   compiling process had registered (sorted), and `load` replays them +
///   marks the registry initialized — the same post-deserialize hydration the
///   Base-cache hit lane performs in `cached_base_from_serialized`, so
///   runtime reflection (`Base.promote_type` family) sees the same registry
///   on the `.sjvmbc` execution path as on a fresh compile.
pub const VERSION: u32 = 7;

/// Upper bound accepted for a length-prefixed fingerprint field. Real
/// fingerprints are 64 hex chars; anything larger means a corrupt header.
const MAX_FINGERPRINT_LEN: u32 = 256;

#[derive(serde::Serialize, serde::Deserialize)]
struct SerializedVmBytecode {
    program: Program,
    compiled: CompiledProgram,
    /// Promotion-registry rules registered while compiling this program
    /// (Issue #10339), sorted for deterministic bytes — replayed on load the
    /// same way the Base-cache hit lane replays its `promotion_rules`
    /// section, so `.sjvmbc` execution does not run reflection against an
    /// empty registry.
    promotion_rules: Vec<(String, String, String)>,
}

/// Persisted VM bytecode file format error.
#[derive(Debug)]
pub enum VmBytecodeFileError {
    /// I/O error during file operations
    IoError(std::io::Error),
    /// Invalid magic bytes - not a valid SubsetJuliaVM VM bytecode file
    InvalidMagic,
    /// File format version differs from this binary's [`VERSION`] (exact
    /// match required — older files are stale, not backward-compatible).
    VersionMismatch(u32),
    /// A header fingerprint differs from this binary's fingerprint: the file
    /// was produced by a different compiler build / wire schema (Issue #10170).
    FingerprintMismatch {
        /// Which fingerprint mismatched: `"schema"`, `"compiler-build"`, or
        /// `"enum-variant"`.
        kind: &'static str,
        expected: String,
        found: String,
    },
    /// Structurally invalid header (e.g. absurd fingerprint length)
    CorruptHeader(String),
    /// Deserialization error
    DeserializeError(String),
    /// Serialization error
    SerializeError(String),
}

impl VmBytecodeFileError {
    /// True when the file is a well-formed `.sjvmbc` that was simply produced
    /// by a different format version or compiler build — the "stale cache"
    /// class a loader (#10171) should treat as a cache miss and regenerate
    /// from the `.jl` source rather than report as corruption.
    pub fn is_stale_cache(&self) -> bool {
        matches!(
            self,
            VmBytecodeFileError::VersionMismatch(_)
                | VmBytecodeFileError::FingerprintMismatch { .. }
        )
    }
}

impl std::fmt::Display for VmBytecodeFileError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            VmBytecodeFileError::IoError(e) => write!(f, "I/O error: {}", e),
            VmBytecodeFileError::InvalidMagic => write!(
                f,
                "Invalid magic bytes - not a valid SubsetJuliaVM VM bytecode file"
            ),
            VmBytecodeFileError::VersionMismatch(v) => {
                write!(
                    f,
                    "VM bytecode file version mismatch: file has version {}, this binary \
                     requires exactly {} — regenerate the .sjvmbc with `--compile-vm`",
                    v, VERSION
                )
            }
            VmBytecodeFileError::FingerprintMismatch {
                kind,
                expected,
                found,
            } => {
                write!(
                    f,
                    "VM bytecode file {} fingerprint mismatch: file was produced by a \
                     different compiler build (file: {}, this binary: {}) — regenerate \
                     the .sjvmbc with `--compile-vm`",
                    kind, found, expected
                )
            }
            VmBytecodeFileError::CorruptHeader(e) => {
                write!(f, "Corrupt VM bytecode file header: {}", e)
            }
            VmBytecodeFileError::DeserializeError(e) => write!(f, "Failed to deserialize: {}", e),
            VmBytecodeFileError::SerializeError(e) => write!(f, "Failed to serialize: {}", e),
        }
    }
}

impl std::error::Error for VmBytecodeFileError {}

impl From<std::io::Error> for VmBytecodeFileError {
    fn from(e: std::io::Error) -> Self {
        VmBytecodeFileError::IoError(e)
    }
}

#[derive(Debug, Clone, Copy, Default)]
struct VmBytecodeFileFlags {
    has_debug_info: bool,
    has_spans: bool,
    _reserved: u16,
}

impl VmBytecodeFileFlags {
    fn default_flags() -> Self {
        Self {
            has_debug_info: true,
            has_spans: true,
            _reserved: 0,
        }
    }

    fn to_u32(self) -> u32 {
        let mut flags: u32 = 0;
        if self.has_debug_info {
            flags |= 1 << 0;
        }
        if self.has_spans {
            flags |= 1 << 1;
        }
        flags
    }
}

/// Header fields written after the magic bytes. Split out so unit tests can
/// construct mismatched headers through the same write path as [`save`].
struct HeaderFields {
    version: u32,
    schema_fingerprint: String,
    compiler_build_fingerprint: String,
    enum_variant_fingerprint: String,
}

impl HeaderFields {
    fn current() -> Self {
        Self {
            version: VERSION,
            schema_fingerprint: crate::compile::precompile::base_cache_schema_fingerprint(),
            compiler_build_fingerprint: crate::compile::precompile::compiler_build_fingerprint()
                .to_string(),
            enum_variant_fingerprint: crate::compile::precompile::enum_variant_fingerprint(),
        }
    }
}

fn write_fingerprint(file: &mut File, fingerprint: &str) -> Result<(), VmBytecodeFileError> {
    file.write_all(&(fingerprint.len() as u32).to_le_bytes())?;
    file.write_all(fingerprint.as_bytes())?;
    Ok(())
}

fn read_fingerprint<R: Read>(
    file: &mut R,
    kind: &'static str,
) -> Result<String, VmBytecodeFileError> {
    let mut len_bytes = [0u8; 4];
    file.read_exact(&mut len_bytes)?;
    let len = u32::from_le_bytes(len_bytes);
    if len > MAX_FINGERPRINT_LEN {
        return Err(VmBytecodeFileError::CorruptHeader(format!(
            "{} fingerprint length {} exceeds maximum {}",
            kind, len, MAX_FINGERPRINT_LEN
        )));
    }
    let mut bytes = vec![0u8; len as usize];
    file.read_exact(&mut bytes)?;
    String::from_utf8(bytes).map_err(|e| {
        VmBytecodeFileError::CorruptHeader(format!("{} fingerprint is not UTF-8: {}", kind, e))
    })
}

fn save_with_header<P: AsRef<Path>>(
    program: &Program,
    compiled: &CompiledProgram,
    path: P,
    header: &HeaderFields,
) -> Result<(), VmBytecodeFileError> {
    let payload = SerializedVmBytecode {
        program: program.clone(),
        compiled: compiled.clone(),
        promotion_rules: {
            // Sorted like `precompile.rs`'s Base-cache section, so identical
            // registries always serialize to identical bytes (Issue #10339).
            let mut rules = crate::promotion::get_all_promotion_rules();
            rules.sort();
            rules
        },
    };
    let payload_bytes = bincode::serialize(&payload)
        .map_err(|e| VmBytecodeFileError::SerializeError(e.to_string()))?;

    let mut file = File::create(path)?;
    file.write_all(MAGIC)?;
    file.write_all(&header.version.to_le_bytes())?;
    file.write_all(&VmBytecodeFileFlags::default_flags().to_u32().to_le_bytes())?;
    write_fingerprint(&mut file, &header.schema_fingerprint)?;
    write_fingerprint(&mut file, &header.compiler_build_fingerprint)?;
    write_fingerprint(&mut file, &header.enum_variant_fingerprint)?;
    file.write_all(&(payload_bytes.len() as u32).to_le_bytes())?;
    file.write_all(&payload_bytes)?;

    Ok(())
}

/// Save a compiled VM program to a VM bytecode file.
pub fn save<P: AsRef<Path>>(
    program: &Program,
    compiled: &CompiledProgram,
    path: P,
) -> Result<(), VmBytecodeFileError> {
    save_with_header(program, compiled, path, &HeaderFields::current())
}

/// Load a compiled VM program from a VM bytecode file.
///
/// Rejects files whose format version is not exactly [`VERSION`] or whose
/// embedded compiler fingerprints differ from this binary's (Issue #10170).
pub fn load<P: AsRef<Path>>(path: P) -> Result<CompiledProgram, VmBytecodeFileError> {
    let file = File::open(path)?;
    load_from_reader(file)
}

/// Load a compiled VM program from in-memory `.sjvmbc` bytes.
///
/// Host bindings (the iOS app reads the bundled sample `.sjvmbc` as a bundle
/// resource) execute a bytecode payload without a filesystem path
/// (Issue #10171). Callers must treat ANY error from this function as a cache
/// miss and fall back to source compilation — the invalidation contract in
/// `docs/vm/CACHE_ARCHITECTURE.md`.
pub fn load_from_bytes(bytes: &[u8]) -> Result<CompiledProgram, VmBytecodeFileError> {
    load_from_reader(bytes)
}

fn load_from_reader<R: Read>(mut file: R) -> Result<CompiledProgram, VmBytecodeFileError> {
    let mut magic = [0u8; 4];
    file.read_exact(&mut magic)?;
    if &magic != MAGIC {
        return Err(VmBytecodeFileError::InvalidMagic);
    }

    let mut version_bytes = [0u8; 4];
    file.read_exact(&mut version_bytes)?;
    let version = u32::from_le_bytes(version_bytes);
    // Exact match: older versions carry payloads laid out for older structs
    // and must be regenerated, not deserialized with current definitions.
    if version != VERSION {
        return Err(VmBytecodeFileError::VersionMismatch(version));
    }

    let mut flags_bytes = [0u8; 4];
    file.read_exact(&mut flags_bytes)?;

    let expected = HeaderFields::current();
    for (kind, expected_fingerprint) in [
        ("schema", expected.schema_fingerprint.as_str()),
        (
            "compiler-build",
            expected.compiler_build_fingerprint.as_str(),
        ),
        ("enum-variant", expected.enum_variant_fingerprint.as_str()),
    ] {
        let found = read_fingerprint(&mut file, kind)?;
        if found != expected_fingerprint {
            return Err(VmBytecodeFileError::FingerprintMismatch {
                kind,
                expected: expected_fingerprint.to_string(),
                found,
            });
        }
    }

    let mut length_bytes = [0u8; 4];
    file.read_exact(&mut length_bytes)?;
    let payload_length = u32::from_le_bytes(length_bytes);

    let mut payload_bytes = vec![0u8; payload_length as usize];
    file.read_exact(&mut payload_bytes)?;

    let payload: SerializedVmBytecode = bincode::deserialize(&payload_bytes)
        .map_err(|e| VmBytecodeFileError::DeserializeError(e.to_string()))?;
    // Post-deserialize hydration beyond `compile_context` (Issue #10339):
    // replay the recorded promotion rules and mark the registry initialized,
    // mirroring the Base-cache hit lane (`cached_base_from_serialized`), so
    // runtime reflection on the `.sjvmbc` execution path does not consult an
    // empty promotion registry. Replaying into an already-populated registry
    // is idempotent for identical rules.
    for (t1, t2, ret) in &payload.promotion_rules {
        crate::promotion::register_promotion_rule(t1, t2, ret);
    }
    crate::promotion::mark_registry_initialized();
    let mut compiled = payload.compiled;
    crate::compile::cache::restore_compile_context_from_program(&mut compiled, &payload.program);
    Ok(compiled)
}

#[cfg(test)]
#[allow(clippy::unwrap_used, clippy::expect_used)]
mod tests {
    use super::*;
    use crate::ir::core::Block;
    use crate::rng::StableRng;
    use crate::span::Span;
    use crate::vm::Vm;

    const TEST_FILE_EXTENSION: &str = "sjvmbc";

    fn empty_block() -> Block {
        Block {
            stmts: vec![],
            span: Span::new(0, 0, 1, 1, 1, 1),
        }
    }

    fn minimal_program() -> Program {
        Program {
            abstract_types: vec![],
            primitive_types: vec![],
            type_aliases: vec![],
            structs: vec![],
            functions: vec![],
            base_function_count: 0,
            modules: vec![],
            usings: vec![],
            macros: vec![],
            enums: vec![],
            main: empty_block(),
        }
    }

    fn minimal_compiled_program() -> CompiledProgram {
        CompiledProgram {
            code: vec![],
            source_map: vec![],
            functions: vec![],
            struct_defs: vec![],
            abstract_types: vec![],
            primitive_types: vec![],
            enum_defs: vec![],
            show_methods: vec![],
            print_methods: vec![],
            entry: 7,
            specializable_functions: vec![],
            runtime_specialization_map: vec![],
            inference_global_types_snapshot: vec![],
            specialization_disable_flags: Default::default(),
            compile_context: None,
            base_function_count: 0,
            macro_bindings: std::collections::HashMap::new(),
            module_registry: Default::default(),
            global_slot_names: vec![],
            global_slot_types: vec![],
            global_slot_count: 0,
            main_scope_names: Default::default(),
        }
    }

    fn temp_path(name: &str) -> std::path::PathBuf {
        let dir = std::env::temp_dir().join(format!(
            "sjvmbc_test_{}_{}",
            std::process::id(),
            std::thread::current()
                .name()
                .unwrap_or("t")
                .replace(':', "_")
        ));
        std::fs::create_dir_all(&dir).expect("create temp dir");
        dir.join(name)
    }

    #[test]
    fn round_trip_save_load_10170() {
        let path = temp_path("round_trip.sjvmbc");
        save(&minimal_program(), &minimal_compiled_program(), &path).expect("save should succeed");

        let compiled = load(&path).expect("load of a freshly saved file should succeed");
        assert_eq!(compiled.entry, 7);
        // `restore_compile_context_from_program` runs on load; for this
        // minimal (empty) program no restored context is needed, so it stays
        // `None` — the point here is that the fingerprinted header round-trips.
        let _ = std::fs::remove_file(&path);
    }

    /// Issue #10339: `.sjvmbc` load must perform the same post-deserialize
    /// promotion hydration the Base-cache hit lane does
    /// (`cached_base_from_serialized`): the rules registered while COMPILING
    /// the program are recorded in the payload at save time and replayed —
    /// with `mark_registry_initialized` — on load, so `.sjvmbc` execution
    /// never runs runtime reflection against an empty registry. The registry
    /// is thread-local and nextest runs one test per process, so the
    /// clear/replay below cannot leak into other tests.
    #[test]
    fn load_replays_promotion_rules_10339() {
        crate::promotion::register_promotion_rule(
            "SjvmbcLhs10339",
            "SjvmbcRhs10339",
            "SjvmbcRet10339",
        );
        let path = temp_path("promotion_replay.sjvmbc");
        save(&minimal_program(), &minimal_compiled_program(), &path).expect("save should succeed");

        // Simulate the consuming process: fresh (empty, uninitialized)
        // registry, exactly what `sjulia file.sjvmbc` starts with.
        crate::promotion::clear_registry();
        assert!(
            !crate::promotion::is_registry_initialized(),
            "precondition: consuming-process registry starts uninitialized"
        );

        load(&path).expect("load of a freshly saved file should succeed");
        let _ = std::fs::remove_file(&path);

        assert!(
            crate::promotion::is_registry_initialized(),
            ".sjvmbc load must mark the promotion registry initialized (Issue #10339)"
        );
        assert!(
            crate::promotion::get_all_promotion_rules().contains(&(
                "SjvmbcLhs10339".to_string(),
                "SjvmbcRhs10339".to_string(),
                "SjvmbcRet10339".to_string()
            )),
            ".sjvmbc load must replay the save-time promotion rules (Issue #10339)"
        );
    }

    #[test]
    fn inference_global_types_survive_sjvmbc_restore_10333() -> Result<(), String> {
        const SOURCE: &str = r#"
const CACHE_CONST_10333 = 41
CACHE_MUT_10333 = 1
read_const_10333() = CACHE_CONST_10333
read_mut_10333() = CACHE_MUT_10333
println(Base.infer_return_type(read_const_10333, Tuple{}))
println(Base.infer_return_type(read_mut_10333, Tuple{}))
println(Base.return_types(read_const_10333, Tuple{}))
println(Base.return_types(read_mut_10333, Tuple{}))
true
"#;
        const EXPECTED: &str = "Int64\nAny\nAny[Int64]\nAny[Any]\n";

        fn run_output(compiled: CompiledProgram) -> Result<String, String> {
            let mut vm = Vm::new_program(compiled, StableRng::new(0));
            vm.run()
                .map_err(|error| format!("reflection corpus should run: {error:?}"))?;
            Ok(vm.get_output().to_string())
        }

        let program = crate::pipeline::parse_and_lower_strict(SOURCE)
            .map_err(|error| format!("reflection corpus should parse and lower: {error:?}"))?;
        let compiled = crate::compile::host_support::compile_with_cache(&program)
            .map_err(|error| format!("reflection corpus should compile: {error:?}"))?;
        assert_eq!(run_output(compiled.clone())?, EXPECTED);

        let path = temp_path(&format!(
            "inference_global_types_10333.{TEST_FILE_EXTENSION}"
        ));
        save(&program, &compiled, &path)
            .map_err(|error| format!("save should succeed: {error}"))?;
        let restored = load(&path).map_err(|error| format!("load should succeed: {error}"))?;
        let _ = std::fs::remove_file(&path);

        assert_eq!(run_output(restored)?, EXPECTED);
        Ok(())
    }

    #[test]
    fn older_version_is_rejected_exactly_10170() {
        let path = temp_path("older_version.sjvmbc");
        let mut header = HeaderFields::current();
        header.version = VERSION - 1;
        save_with_header(
            &minimal_program(),
            &minimal_compiled_program(),
            &path,
            &header,
        )
        .expect("save should succeed");

        let err = load(&path).expect_err("an older-version file must be rejected");
        assert!(
            matches!(err, VmBytecodeFileError::VersionMismatch(v) if v == VERSION - 1),
            "expected VersionMismatch({}), got: {err:?}",
            VERSION - 1
        );
        assert!(
            err.is_stale_cache(),
            "version mismatch is a stale-cache error"
        );
        let message = err.to_string();
        assert!(
            message.contains("version mismatch") && message.contains("--compile-vm"),
            "error message should be actionable, got: {message}"
        );
        let _ = std::fs::remove_file(&path);
    }

    #[test]
    fn newer_version_is_rejected_10170() {
        let path = temp_path("newer_version.sjvmbc");
        let mut header = HeaderFields::current();
        header.version = VERSION + 1;
        save_with_header(
            &minimal_program(),
            &minimal_compiled_program(),
            &path,
            &header,
        )
        .expect("save should succeed");

        let err = load(&path).expect_err("a newer-version file must be rejected");
        assert!(
            matches!(err, VmBytecodeFileError::VersionMismatch(v) if v == VERSION + 1),
            "expected VersionMismatch({}), got: {err:?}",
            VERSION + 1
        );
        let _ = std::fs::remove_file(&path);
    }

    #[test]
    fn tampered_schema_fingerprint_is_rejected_10170() {
        let path = temp_path("tampered_schema.sjvmbc");
        let mut header = HeaderFields::current();
        header.schema_fingerprint = format!("{}0", header.schema_fingerprint);
        save_with_header(
            &minimal_program(),
            &minimal_compiled_program(),
            &path,
            &header,
        )
        .expect("save should succeed");

        let err = load(&path).expect_err("a schema-fingerprint mismatch must be rejected");
        assert!(
            matches!(
                err,
                VmBytecodeFileError::FingerprintMismatch { kind: "schema", .. }
            ),
            "expected schema FingerprintMismatch, got: {err:?}"
        );
        assert!(err.is_stale_cache());
        let _ = std::fs::remove_file(&path);
    }

    #[test]
    fn tampered_compiler_build_fingerprint_is_rejected_10170() {
        let path = temp_path("tampered_build.sjvmbc");
        let mut header = HeaderFields::current();
        header.compiler_build_fingerprint = "deadbeef".to_string();
        save_with_header(
            &minimal_program(),
            &minimal_compiled_program(),
            &path,
            &header,
        )
        .expect("save should succeed");

        let err = load(&path).expect_err("a compiler-build fingerprint mismatch must be rejected");
        assert!(
            matches!(
                err,
                VmBytecodeFileError::FingerprintMismatch {
                    kind: "compiler-build",
                    ..
                }
            ),
            "expected compiler-build FingerprintMismatch, got: {err:?}"
        );
        let _ = std::fs::remove_file(&path);
    }

    #[test]
    fn tampered_enum_variant_fingerprint_is_rejected_10170() {
        // Mirrors the Base cache's
        // `mismatched_enum_fingerprint_cache_is_discarded_and_removed_8626`.
        let path = temp_path("tampered_enum.sjvmbc");
        let mut header = HeaderFields::current();
        header.enum_variant_fingerprint =
            "0000000000000000000000000000000000000000000000000000000000000000".to_string();
        save_with_header(
            &minimal_program(),
            &minimal_compiled_program(),
            &path,
            &header,
        )
        .expect("save should succeed");

        let err = load(&path).expect_err("an enum-variant fingerprint mismatch must be rejected");
        assert!(
            matches!(
                err,
                VmBytecodeFileError::FingerprintMismatch {
                    kind: "enum-variant",
                    ..
                }
            ),
            "expected enum-variant FingerprintMismatch, got: {err:?}"
        );
        let _ = std::fs::remove_file(&path);
    }

    #[test]
    fn invalid_magic_is_rejected_10170() {
        let path = temp_path("invalid_magic.sjvmbc");
        std::fs::write(&path, b"NOPE0000000000000000").expect("write file");

        let err = load(&path).expect_err("a non-.sjvmbc file must be rejected");
        assert!(matches!(err, VmBytecodeFileError::InvalidMagic));
        assert!(
            !err.is_stale_cache(),
            "invalid magic is corruption, not a stale cache"
        );
        let _ = std::fs::remove_file(&path);
    }

    #[test]
    fn truncated_header_is_rejected_10170() {
        let path = temp_path("truncated.sjvmbc");
        save(&minimal_program(), &minimal_compiled_program(), &path).expect("save should succeed");
        let bytes = std::fs::read(&path).expect("read file");
        // Keep magic + version + flags but cut into the fingerprint block.
        std::fs::write(&path, &bytes[..14]).expect("truncate file");

        let err = load(&path).expect_err("a truncated header must be rejected");
        assert!(
            matches!(err, VmBytecodeFileError::IoError(_)),
            "expected IoError for truncated header, got: {err:?}"
        );
        let _ = std::fs::remove_file(&path);
    }

    #[test]
    fn oversized_fingerprint_length_is_corrupt_header_10170() {
        let path = temp_path("oversized_fp.sjvmbc");
        let mut bytes = Vec::new();
        bytes.extend_from_slice(MAGIC);
        bytes.extend_from_slice(&VERSION.to_le_bytes());
        bytes.extend_from_slice(&0u32.to_le_bytes()); // flags
        bytes.extend_from_slice(&u32::MAX.to_le_bytes()); // absurd fingerprint length
        std::fs::write(&path, &bytes).expect("write file");

        let err = load(&path).expect_err("an absurd fingerprint length must be rejected");
        assert!(
            matches!(err, VmBytecodeFileError::CorruptHeader(_)),
            "expected CorruptHeader, got: {err:?}"
        );
        let _ = std::fs::remove_file(&path);
    }

    /// Issue #10906 (Phase 1c of #10869): a `.sjvmbc` whose HEADER validates
    /// (right magic/version/fingerprints) but whose bincode PAYLOAD is
    /// truncated/bit-flipped must never panic the host — the "cache
    /// deserialize/load" boundary is explicitly named as an entrypoint in
    /// #10869. Corrupts a run of bytes deep inside the payload (past the
    /// header this file already validates before touching the payload) and
    /// asserts the load never panics; if it does return an error, that error
    /// must be the typed `DeserializeError`/`IoError` variant, never some
    /// other failure mode.
    #[test]
    fn corrupted_payload_bytes_never_panic_10906() {
        let path = temp_path("corrupted_payload.sjvmbc");
        let program = crate::pipeline::parse_and_lower_strict(
            "function f10906(x)\n    x + 1\nend\nprintln(f10906(41))\n",
        )
        .expect("parse/lower should succeed");
        let compiled = crate::compile::host_support::compile_with_cache(&program)
            .expect("compile should succeed");
        save(&program, &compiled, &path).expect("save should succeed");

        let bytes = std::fs::read(&path).expect("read file");
        let header = HeaderFields::current();
        let header_len = 4 // magic
            + 4 // version
            + 4 // flags
            + (4 + header.schema_fingerprint.len())
            + (4 + header.compiler_build_fingerprint.len())
            + (4 + header.enum_variant_fingerprint.len())
            + 4; // payload length prefix
        assert!(
            bytes.len() > header_len + 32,
            "test program's payload is too small to exercise payload corruption: \
             {} bytes total (header {} bytes)",
            bytes.len(),
            header_len
        );

        // Corrupt a run of bytes squarely inside the payload region, past
        // the header this file already validates before ever touching the
        // payload.
        let corrupt_at = header_len + (bytes.len() - header_len) / 2;
        let mut corrupted = bytes.clone();
        for b in corrupted.iter_mut().skip(corrupt_at).take(32) {
            *b ^= 0xFF;
        }
        std::fs::write(&path, &corrupted).expect("write corrupted file");

        let result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| load(&path)));
        assert!(
            result.is_ok(),
            "loading a corrupted .sjvmbc payload must never panic (Issue #10906)"
        );
        if let Ok(Err(e)) = &result {
            assert!(
                matches!(
                    e,
                    VmBytecodeFileError::DeserializeError(_) | VmBytecodeFileError::IoError(_)
                ),
                "expected a deserialize/IO error for a corrupted payload, got: {e:?}"
            );
        }
        let _ = std::fs::remove_file(&path);
    }

    #[test]
    fn compiler_build_fingerprint_covers_payload_dependency_crates_10332() {
        // The .sjvmbc payload (SerializedVmBytecode) carries serde-derived
        // types from sibling crates: `Program` embeds `Expr`/`JuliaType`/
        // `TypeExpr`/`TypeParam` (subset_julia_vm_types) and `Span`
        // (subset_julia_vm_ir); `CompiledProgram` lives in
        // subset_julia_vm_bytecode. Those files are NOT all in the Base-cache
        // schema manifest, so the compiler-build fingerprint embedded in the
        // header must hash every one of these crates' src trees — otherwise a
        // serde-shape change there (e.g. a JuliaType variant reorder) leaves
        // all header fingerprints unchanged and the stale payload is
        // misdecoded (Issue #10332, found via codex review of PR #10328).
        // `SJULIA_CACHE_BUILD_FINGERPRINT_ROOTS` is emitted by build.rs from
        // the exact root list it hashes.
        let roots: Vec<&str> = env!("SJULIA_CACHE_BUILD_FINGERPRINT_ROOTS")
            .split(',')
            .collect();
        for required in [
            "src",
            "../subset_julia_vm_ir/src",
            "../subset_julia_vm_types/src",
            "../subset_julia_vm_bytecode/src",
        ] {
            assert!(
                roots.contains(&required),
                "compiler build fingerprint must hash {required} (a crate whose types \
                 are serialized in .sjvmbc payloads); currently hashed roots: {roots:?}"
            );
        }
    }
}
