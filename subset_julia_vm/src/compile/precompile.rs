//! Precompiled Base cache serialization.
//!
//! Provides save/load for the `SerializedBaseCache`, which contains
//! all data needed to skip Base compilation at startup.

use bincode::Options;
use serde::{de::DeserializeOwned, Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::collections::{HashMap, HashSet};

use crate::vm::CompiledProgram;

use super::abstract_interp::engine::{CachedReturn, InferenceCacheKey};
use super::MethodTable;

/// Version of the cache format. Increment on breaking changes.
///
/// Bumped to 66 for Issue #8444: the Base cache envelope now stores the
/// bytecode/method-table schema fingerprint and compiler build fingerprint so
/// stale caches are rejected before payload decode even if the manual version
/// bump is missed.
///
/// Bumped to 65 for Issue #6752: the `BuiltinId::Isnumeric` variant was removed
/// (isnumeric is now Pure Julia). As with #7875, removing a mid-enum `BuiltinId`
/// shifts the bincode discriminants of later variants, so older cached
/// `CompiledProgram`s must be invalidated.
///
/// Bumped to 64 for Issue #7875: the `BuiltinId::StringToIntBase` variant was
/// removed (parse(Int, s; base=N) is now Pure Julia). Removing a mid-enum
/// `BuiltinId` shifts the bincode discriminants of all later variants, so any
/// `Instr::CallBuiltin` in an older cached `CompiledProgram` would deserialize
/// to the wrong builtin; the version bump invalidates those caches.
///
/// Bumped to 62 for Issue #7357: persistent/embedded Base caches persist
/// `specializable_functions` plus `runtime_specialization_map`, so warm
/// compilation can restore cached Base `CallSpecialize` metadata instead of
/// rescanning every Base function on WASM.
///
/// Bumped to 47 for Issue #6453: all Base cache sections now use varint bincode
/// encoding (`cache_codec`) instead of the default fixint encoding, which
/// changes the wire layout of every section. See `cache_codec` for the
/// rationale; the decoded data is bit-identical.
///
/// Bumped to 46 for Issue #6496: the `Instr::CallDynamic` candidate payload
/// changed from `Vec<(usize, String)>` (function index + baked expected type
/// name, with `usize::MAX` + native type-name sentinels) to the structured
/// `Vec<DynamicCallCandidate>` (`Method(index)` / `NativeIterator(kind)`),
/// changing the bincode layout of serialized bytecode. The runtime derives
/// each method candidate's expected type name from its `FunctionInfo`.
/// The same migration (still version 46, same branch) also replaced the baked
/// name-string payloads of `Instr::CallDynamicBinary` /
/// `CallDynamicBinaryBoth` / `CallDynamicBinaryNoFallback` /
/// `CallDynamicOrBuiltin` with index-only `Vec<usize>` candidates.
///
/// Bumped to 45 for Issue #6336 (final phase): `MethodSig` now serializes a
/// dedicated wire format (`MethodSigWire`) in which `core_signature` is the
/// canonical (and only) type representation plus display `param_names`; the
/// legacy `params` / `type_params` fields are no longer serialized and are
/// reconstructed from `core_signature` on deserialization. Older caches carry
/// the old field layout and must be regenerated.
///
/// Bumped to 44 for Issue #6336: `Instr::IterateDynamic`'s candidate payload
/// changed from `Vec<(usize, String)>` (function index + `\u{1f}`-joined
/// type-name signature) to the structured `Vec<usize>` candidate indices,
/// changing the bincode layout of serialized bytecode. The runtime derives the
/// name-pattern fallback signature from each candidate's `FunctionInfo`.
///
/// Bumped to 43 for Issue #6449: the Base-cache `CompiledProgram` payload is
/// split into sub-sections so `code`, function metadata, specialization IR, and
/// global-slot metadata can be timed independently. Persistent/embedded Base
/// caches also stop persisting `specializable_functions`; warm compilation
/// rebuilds those registrations from the prelude/user Program, and the cached
/// Base `CompiledProgram` only supplies code/function metadata as a prefix.
///
/// Bumped to 42 for Issue #6440: the outer Base cache payload is now a small
/// section envelope. Each major payload remains bincode-encoded, but decode can
/// time `compiled`, method tables, closure captures, promotion rules, and
/// inference results separately. Method tables also stop serializing per-table
/// hierarchy projection maps that compile setup immediately rebuilds.
///
/// Bumped to 41 for Issue #6348 follow-up: persistent/embedded Base caches no
/// longer persist inference return snapshots. Cached Base functions are skipped
/// on warm compilation, and replaying the full Base return cache made later user
/// method additions pay broad invalidation costs.
///
/// Bumped to 40 for Issue #6272 / Issue #6251 follow-up: reflection exception
/// classification and Base numeric specializations changed after main's v24
/// cache line, so stale Base bytecode must be regenerated.
///
/// Bumped to 39 for Issue #6251 follow-up: Base mixed `Int64`/`Rational{Int64}`
/// `rem`/`mod` gained concrete specializations.
///
/// Bumped to 38 for Issue #6251 follow-up: Base narrow `Rational` `fld`/`cld`
/// specializations cast generic integer rounding results back to field type.
///
/// Bumped to 37 for Issue #6251 follow-up: Base `Rational{Int32/Int16/Int8}`
/// `rem`/`mod` gained concrete specializations.
///
/// Bumped to 36 for Issue #6251 follow-up: Base narrow `Rational` `div`/`fld`/`cld`
/// specializations now cast widened intermediate products back to their field type.
///
/// Bumped to 35 for Issue #6251 follow-up: Base `Rational{Int32/Int16/Int8}`
/// `div`/`fld`/`cld` gained concrete specializations.
///
/// Bumped to 34 for Issue #6251 follow-up: Base `Rational{Int32/Int16/Int8}`
/// unary `-` and `inv` gained concrete specializations.
///
/// Bumped to 33 for Issue #6251 follow-up: matrix/vector multiplication
/// bytecode now keeps all linalg array candidates when ValueType lacks rank.
///
/// Bumped to 32 for Issue #6251 follow-up: reciprocal inverse trig
/// wrappers now declare Float64 returns so call sites do not widen to Any.
///
/// Bumped to 31 for Issue #6251 follow-up: Base Irrational 2-arg
/// `isapprox` entry methods avoid keyword-default slots on dynamic dispatch.
///
/// Bumped to 30 for Issue #6251 follow-up: Base Irrational `isapprox`
/// added Any-side entry methods for values whose compile-time type is unknown.
///
/// Bumped to 29 for Issue #6251 follow-up: Base Irrational `isapprox`
/// gained Float64/AbstractIrrational entry specializations that bypass broad
/// `_isapprox_scalar` dispatch.
///
/// Bumped to 28 for Issue #6251 follow-up: Base Irrational `isapprox`
/// now routes converted Float64 pairs through an intrinsic-only helper.
///
/// Bumped to 27 for Issue #6251 follow-up: Base `_isapprox_scalar`
/// AbstractIrrational forwarding now computes directly after Float64
/// conversion so stale cached methods do not recurse through broad dispatch.
///
/// Bumped to 26 for Issue #6251 follow-up: Base `Rational{Int32/Int16/Int8}`
/// arithmetic gained concrete pure-Julia specializations so narrow Rational
/// field types are not widened by stale cached generic Rational bytecode.
///
/// Bumped to 24 for Issue #6251 follow-up: Base bytecode for tuple-shaped
/// `similar(a, dims::Tuple)` calls must be regenerated so cached `getindex`
/// bodies route through tuple-dims dispatch instead of an older builtin/vararg
/// fallback compiled before the runtime dispatch fix.
///
/// Issue #6270 also requires this cache line: binary operator compilation for
/// bare parametric struct annotations now preserves the UnionAll-shaped
/// JuliaType and avoids over-specific static Base dispatch. Older Base bytecode
/// can contain direct calls such as `*(Rational{BigInt}, Rational{BigInt})` for
/// `x::Rational` bodies and must be regenerated.
///
/// Bumped to 23 for Issue #5968 / #5967: `CachedReturn` (persisted inside
/// `inference_results`) gained a `method_edges` field inserted before
/// `global_reads`. bincode is a *positional* format, so `#[serde(default)]`
/// cannot reconstruct the missing field from an older 4-field snapshot — the
/// later fields misalign and the entry decodes into garbage or a cryptic
/// `bincode` error. #5967 added the field without bumping this version, so a
/// v22 snapshot built between #5582 and #5967 carries the old layout under the
/// same version number. The bump (plus the up-front version gate in
/// `deserialize_base_cache`) rejects every such snapshot cleanly so it is
/// regenerated rather than misdecoded.
///
/// Bumped to 22 for Issue #4708/#5582 follow-up: compile-time subtype
/// dispatch around CoreType-backed user abstracts changed semantics. Older
/// caches may contain Base bytecode compiled under stale dispatch choices.
///
/// Bumped to 21 for Issue #3025 follow-up: promotion-rule extraction now
/// recognizes statically recoverable `Expr::DynamicTypeConstruct` return
/// bodies such as `Rational{Int64}`. Older caches may miss those rules even
/// though their serialized layout still decodes.
///
/// Bumped to 18 for Issue #5058: `CompiledProgram` gained a `primitive_types`
/// field (and the new `PrimitiveTypeDefInfo` type) to support user
/// `primitive type Name Bits end` declarations, changing the bincode layout.
///
/// Bumped to 17 for Issue #5139: the new `Instr::RegisterEnum` /
/// `Instr::ConstructEnum` variants (and the `RegisterEnumOperands` payload)
/// were appended to the `Instr` enum, changing its bincode layout.
///
/// Bumped to 16 for Issue #5112: the `Expr::DynamicTypeConstruct` IR node
/// gained a `splat_mask` field and a new `Instr::ConstructParametricTypeSplat`
/// variant was appended, both of which change the bincode layout.
// Bumped to 48 for Issue #6722: the BuiltinId variants CountZeros / LeadingOnes
// / TrailingOnes / Bitrotate were removed (now pure Julia), shifting the bincode
// discriminants of every later variant in `Instr::CallBuiltin` payloads.
// Bumped to 49 for Issue #6724: removed the dead BuiltinId variants
// UnescapeString / StringCount / StringFindAll (now pure Julia), again shifting
// later `BuiltinId` discriminants.
// Bumped to 50 for Issue #6745: removed the dead (already pure-Julia) BuiltinId
// variants Prod / Minimum / Maximum / Argmin / Argmax / FindFirst / FindAll.
// Bumped to 51 for Issue #6737: removed the BuiltinId::Widemul variant (widemul
// is now Pure Julia), shifting later `BuiltinId` discriminants.
// Bumped to 52 for Issue #6748: removed the BuiltinId::StringToFloat variant
// (parse(Float64,s) is now pure Julia), shifting later discriminants.
// Bumped to 53 for Issue #6738: removed BuiltinId Isbits/Ismutable/Hasfield (now pure Julia).
// Bumped to 54 for Issue #6740: removed BuiltinId NextFloat/PrevFloat/NextFloatN/PrevFloatN/Exponent/Significand/Frexp/Issubnormal (now pure Julia).
// Bumped to 55 for Issue #6747: removed BuiltinId Codepoint/Bitstring (now pure Julia).
// Bumped to 56 for Issue #6742: appended the Intrinsic::RintLlvm variant (round to
// nearest, ties to even) for pure-Julia round; the new variant shifts the bincode
// discriminants of later `Intrinsic` variants in compiled-bytecode payloads.
// Bumped to 57 for Issue #6746: added the BuiltinId::PrintfFmtFloat variant (the
// C float→string boundary for the pure-Julia Printf engine), shifting later
// `BuiltinId` discriminants in compiled-bytecode payloads.
// Bumped to 58 for Issue #6733: removed the dead legacy reducer HOF Instr variants
// (FindAllFunc/FindFirstFunc/FindLastFunc/MapReduceFunc(WithInit)/MapFoldrFunc(WithInit)/
// MapFuncInPlace/FilterFuncInPlace/SumFunc/AnyFunc/AllFunc/CountFunc), shifting the
// bincode discriminants of later `Instr` variants in compiled-bytecode payloads.
// Bumped to 59 for Issue #6731: removed the `Value::Dict` carrier and its
// `BuiltinId`s (`_DictGet`/`_DictSet`/…/`_DictPairs`, `DictNew`, `DictMerge`,
// `DictLen`), shifting later `BuiltinId` discriminants in compiled-bytecode payloads.
// Bumped to 60 for Issue #6732: removed the `Value::Set` carrier and its
// `BuiltinId`s (`SetNew`/`SetPush`/`SetDelete`/`SetIn`/`SetEmpty`, `_SetPush`..
// `_SetLength`), shifting later `BuiltinId` discriminants in compiled-bytecode
// payloads. The Set Instrs (`NewSet`/`LoadSet`/…) are kept decodable but
// unreachable (their handlers now error), so Instr discriminants are unchanged.
// Bumped to 61 for Issue #6728: `hash` is no longer force-intercepted to
// `BuiltinId::Hash`; it dispatches through pure-Julia `hash` methods. This is a
// compiler-only change (no enum discriminant shift) but it alters the compiled
// base bytecode for `hash` calls, so the cached base must be regenerated for
// base-internal `hash` (e.g. Dict/Set `hashindex`) to respect user `hash(::T)`.
const CACHE_VERSION: u32 = 66;
const BASE_CACHE_MAGIC: [u8; 8] = *b"SJBCACH1";
const SECTION_LEN_BYTES: usize = std::mem::size_of::<u64>();

/// Bincode codec for every Base cache section payload (Issue #6453).
///
/// The default `bincode::serialize`/`deserialize` free functions use *fixint*
/// encoding: each `Instr` enum carries a 4-byte u32 discriminant and each
/// `usize` operand a full 8 bytes. Base bytecode is ~78k instructions of mostly
/// small operands (`LoadSlot`, `PushI64`, …), so fixint dominates the payload.
/// Varint encoding shrinks the `compiled.code` section ~70% and
/// `compiled.functions` ~66% — a wire-format change only, the decoded data is
/// bit-identical. `allow_trailing_bytes` preserves the streaming version-gate
/// read in [`deserialize_base_cache`], where a `deserialize_from` over the whole
/// buffer reads just the leading header and ignores the rest. The section
/// framing (`u64` length prefixes in `append_section_bytes`/`read_section`) is
/// written/read as raw little-endian bytes and is independent of this codec.
fn cache_codec() -> impl Options {
    bincode::DefaultOptions::new()
        .with_varint_encoding()
        .allow_trailing_bytes()
}

/// Serialized Base cache containing all precompiled data.
///
/// `method_tables`, `closure_captures`, and `promotion_rules` are emitted in a
/// deterministic key order — see [`super::sorted_serde`] for why bytewise
/// stability matters here (build-time `include_bytes!` dependency tracking).
#[derive(Debug, Serialize, Deserialize)]
pub(crate) struct SerializedBaseCache {
    pub(crate) version: u32,
    /// SHA-256 of get_prelude() source plus the compiler/VM build fingerprint.
    pub(crate) source_hash: String,
    pub(crate) compiled: CompiledProgram,
    #[serde(serialize_with = "super::sorted_serde::sorted_hashmap")]
    pub(crate) method_tables: HashMap<String, MethodTable>,
    #[serde(serialize_with = "super::sorted_serde::sorted_hashmap_of_hashset")]
    pub(crate) closure_captures: HashMap<String, HashSet<String>>,
    /// Promotion rules extracted from method_tables: (type1, type2, result).
    /// Sorted lexicographically by `serialize_base_cache` so the on-disk
    /// representation does not depend on HashMap iteration order.
    pub(crate) promotion_rules: Vec<(String, String, String)>,
    /// Inference return cache entries captured after Base source compilation
    /// (Issue #5093). Entries are already emitted in deterministic order by
    /// `InferenceEngine::snapshot_return_cache`.
    pub(crate) inference_results: Vec<(InferenceCacheKey, CachedReturn)>,
}

#[derive(Serialize, Deserialize)]
struct CacheEnvelopeHeader {
    version: u32,
    magic: [u8; 8],
    source_hash: String,
    schema_fingerprint: String,
    compiler_build_fingerprint: String,
}

#[derive(Serialize, Deserialize)]
struct CompiledProgramHeader {
    entry: usize,
    base_function_count: usize,
    global_slot_count: usize,
}

/// Compute SHA-256 of the prelude source for staleness detection.
///
/// The prelude source is fixed at build time (`include_str!`), but
/// `get_prelude()` re-concatenates a multi-MB string per call and SHA-256
/// over it costs ~3.4 ms — and warm starts used to do this 3-4 times per run
/// (prelude cache path + verify, base cache verify). Memoize it once per
/// process (Issue #6348).
pub(crate) fn compute_prelude_hash() -> String {
    static PRELUDE_HASH: once_cell::sync::Lazy<String> = once_cell::sync::Lazy::new(|| {
        let prelude_src = crate::base::get_prelude();
        let mut hasher = Sha256::new();
        hasher.update(prelude_src.as_bytes());
        format!("{:x}", hasher.finalize())
    });
    PRELUDE_HASH.clone()
}

/// Compute the Base bytecode cache compatibility hash.
///
/// Base cache correctness depends on both the Julia Base source and the Rust
/// compiler/VM code that turns that source into bytecode. The build script
/// fingerprints the crate's Rust sources into `SJULIA_BASE_CACHE_BUILD_HASH`
/// and hashes schema-sensitive source files into
/// `SJULIA_BASE_CACHE_SCHEMA_HASH`; combine them with the Base/prelude source
/// hash so persistent and embedded caches miss after compiler/runtime changes
/// even when Base source is unchanged (Issues #7515/#8444).
pub(crate) fn compute_base_cache_hash() -> String {
    static BASE_CACHE_HASH: once_cell::sync::Lazy<String> = once_cell::sync::Lazy::new(|| {
        let mut hasher = Sha256::new();
        hasher.update(compute_prelude_hash().as_bytes());
        hasher.update(b"\0schema\0");
        hasher.update(base_cache_schema_fingerprint().as_bytes());
        hasher.update(b"\0compiler\0");
        hasher.update(compiler_build_fingerprint().as_bytes());
        format!("{:x}", hasher.finalize())
    });
    BASE_CACHE_HASH.clone()
}

/// Fingerprint of Rust wire-format inputs that affect serialized Base bytecode.
///
/// The build script hashes the explicit manifest in
/// `base_cache_schema_files.txt`, covering bytecode instructions, method-table
/// wire structs, VM type metadata, and inference-cache keys (Issue #8444).
pub(crate) fn base_cache_schema_fingerprint() -> String {
    env!("SJULIA_BASE_CACHE_SCHEMA_HASH").to_string()
}

fn compiler_build_fingerprint() -> &'static str {
    env!("SJULIA_BASE_CACHE_BUILD_HASH")
}

/// Serialize the Base cache to bytes.
pub(crate) fn serialize_base_cache(
    compiled: &CompiledProgram,
    method_tables: &HashMap<String, MethodTable>,
    closure_captures: &HashMap<String, HashSet<String>>,
    _inference_results: &[(InferenceCacheKey, CachedReturn)],
) -> Result<Vec<u8>, String> {
    // Use the promotion rules already registered in the thread-local registry (Issue #3025).
    // The old approach (extract_promotion_rules_for_cache) used method_table return types
    // which are always ValueType::Any after inference, yielding zero rules.
    // The new approach reads from the registry populated by extract_promotion_rules_from_ir
    // during compile_base_functions().
    let mut promotion_rules = super::promotion::get_all_promotion_rules();
    // get_all_promotion_rules iterates a HashMap; sort so the on-disk
    // representation is independent of the per-process hash seed.
    promotion_rules.sort();

    let header = CacheEnvelopeHeader {
        version: CACHE_VERSION,
        magic: BASE_CACHE_MAGIC,
        source_hash: compute_base_cache_hash(),
        schema_fingerprint: base_cache_schema_fingerprint(),
        compiler_build_fingerprint: compiler_build_fingerprint().to_string(),
    };

    let mut bytes = cache_codec()
        .serialize(&header)
        .map_err(|e| format!("Base cache header serialization failed: {}", e))?;
    append_compiled_program_section(&mut bytes, compiled)?;
    append_section(&mut bytes, "method_tables", method_tables)?;
    append_section(&mut bytes, "closure_captures", closure_captures)?;
    append_section(&mut bytes, "promotion_rules", &promotion_rules)?;
    // Persistent/embedded Base caches keep this field empty intentionally.
    // Same-process source-compiled Base cache hits still keep their in-memory
    // inference snapshot in `CachedBase`, but serializing it costs startup
    // twice: decode time plus seeded-cache invalidation when user methods are
    // added. See Issue #6348.
    append_section(
        &mut bytes,
        "inference_results",
        &Vec::<(InferenceCacheKey, CachedReturn)>::new(),
    )?;

    Ok(bytes)
}

/// A prefix of [`SerializedBaseCache`] holding just the leading `version` field.
///
/// Read first — via a streaming `deserialize_from`, which (unlike
/// `bincode::deserialize`) does not require consuming the whole buffer — so an
/// incompatible older snapshot is rejected at the version gate *before* the full
/// positional decode. bincode is positional, so a later layout change (e.g.
/// `CachedReturn.method_edges`, #5967) would otherwise misalign every following
/// field and surface as a cryptic decode error or silent garbage (Issue #5968).
/// `version` is the cache envelope's first field, so this reads the same
/// leading bytes without decoding the rest of the envelope or payload.
#[derive(Serialize, Deserialize)]
struct CacheVersionHeader {
    version: u32,
}

/// Deserialize and validate a Base cache from bytes.
pub(crate) fn deserialize_base_cache(bytes: &[u8]) -> Result<SerializedBaseCache, String> {
    // Gate on the version BEFORE the full decode (Issue #5968). Decoding the
    // whole struct first would let an older snapshot whose later fields changed
    // layout misdecode into garbage (caught only by luck) or a cryptic
    // "Deserialization failed", instead of a clean version rejection.
    let header: CacheVersionHeader = super::profile::time("cache.deserialize_header", || {
        cache_codec().deserialize_from(std::io::Cursor::new(bytes))
    })
    .map_err(|e| format!("Cache header read failed: {}", e))?;
    if header.version != CACHE_VERSION {
        return Err(format!(
            "Cache version mismatch: expected {}, got {}",
            CACHE_VERSION, header.version
        ));
    }

    let mut cursor = std::io::Cursor::new(bytes);
    let header: CacheEnvelopeHeader = super::profile::time("cache.deserialize_envelope", || {
        cache_codec().deserialize_from(&mut cursor)
    })
    .map_err(|e| format!("Cache envelope read failed: {}", e))?;
    if header.magic != BASE_CACHE_MAGIC {
        return Err("Cache format mismatch: unsupported Base cache envelope".to_string());
    }

    // Defensive: the envelope version must agree with the gated header (always
    // true for well-formed bytes; kept as a guard against future drift).
    if header.version != CACHE_VERSION {
        return Err(format!(
            "Cache version mismatch: expected {}, got {}",
            CACHE_VERSION, header.version
        ));
    }
    let current_schema_fingerprint = base_cache_schema_fingerprint();
    if header.schema_fingerprint != current_schema_fingerprint {
        return Err(format!(
            "Base cache schema fingerprint mismatch: expected {}, got {}",
            current_schema_fingerprint, header.schema_fingerprint
        ));
    }
    if header.compiler_build_fingerprint != compiler_build_fingerprint() {
        return Err(
            "Compiler build fingerprint mismatch: cache was built by a different compiler build"
                .to_string(),
        );
    }

    let mut offset = cursor.position() as usize;
    let compiled = deserialize_compiled_program_section(
        bytes,
        &mut offset,
        "compiled",
        "cache.deserialize.compiled",
        "cache.section.compiled_bytes",
    )?;
    let method_tables = deserialize_section(
        bytes,
        &mut offset,
        "method_tables",
        "cache.deserialize.method_tables",
        "cache.section.method_tables_bytes",
    )?;
    let closure_captures = deserialize_section(
        bytes,
        &mut offset,
        "closure_captures",
        "cache.deserialize.closure_captures",
        "cache.section.closure_captures_bytes",
    )?;
    let promotion_rules = deserialize_section(
        bytes,
        &mut offset,
        "promotion_rules",
        "cache.deserialize.promotion_rules",
        "cache.section.promotion_rules_bytes",
    )?;
    let inference_results = deserialize_section(
        bytes,
        &mut offset,
        "inference_results",
        "cache.deserialize.inference_results",
        "cache.section.inference_results_bytes",
    )?;
    if offset != bytes.len() {
        return Err(format!(
            "Cache has {} trailing bytes after section decode",
            bytes.len() - offset
        ));
    }

    let cache = SerializedBaseCache {
        version: header.version,
        source_hash: header.source_hash,
        compiled,
        method_tables,
        closure_captures,
        promotion_rules,
        inference_results,
    };

    let current_hash =
        super::profile::time("cache.compute_base_cache_hash", compute_base_cache_hash);
    if cache.source_hash != current_hash {
        return Err(
            "Source hash mismatch: cache was built with different Base source or compiler"
                .to_string(),
        );
    }

    record_cache_profile(&cache, bytes.len());

    Ok(cache)
}

fn append_compiled_program_section(
    bytes: &mut Vec<u8>,
    compiled: &CompiledProgram,
) -> Result<(), String> {
    let mut payload = cache_codec()
        .serialize(&CompiledProgramHeader {
            entry: compiled.entry,
            base_function_count: compiled.base_function_count,
            global_slot_count: compiled.global_slot_count,
        })
        .map_err(|e| format!("Base cache compiled header serialization failed: {e}"))?;

    append_section(&mut payload, "compiled.code", &compiled.code)?;
    append_section(&mut payload, "compiled.functions", &compiled.functions)?;
    append_section(&mut payload, "compiled.struct_defs", &compiled.struct_defs)?;
    append_section(
        &mut payload,
        "compiled.abstract_types",
        &compiled.abstract_types,
    )?;
    append_section(
        &mut payload,
        "compiled.primitive_types",
        &compiled.primitive_types,
    )?;
    append_section(
        &mut payload,
        "compiled.show_methods",
        &compiled.show_methods,
    )?;
    append_section(
        &mut payload,
        "compiled.specializable_functions",
        &compiled.specializable_functions,
    )?;
    append_section(
        &mut payload,
        "compiled.runtime_specialization_map",
        &compiled.runtime_specialization_map,
    )?;
    append_section(
        &mut payload,
        "compiled.global_slot_names",
        &compiled.global_slot_names,
    )?;
    append_section(
        &mut payload,
        "compiled.global_slot_types",
        &compiled.global_slot_types,
    )?;

    append_section_bytes(bytes, "compiled", &payload)
}

fn deserialize_compiled_program_section(
    bytes: &[u8],
    offset: &mut usize,
    section_name: &'static str,
    time_label: &'static str,
    bytes_label: &'static str,
) -> Result<CompiledProgram, String> {
    let section = read_section(bytes, offset, section_name, bytes_label)?;
    super::profile::time(time_label, || deserialize_compiled_program_body(section))
}

fn deserialize_compiled_program_body(bytes: &[u8]) -> Result<CompiledProgram, String> {
    let mut cursor = std::io::Cursor::new(bytes);
    let header: CompiledProgramHeader =
        super::profile::time("cache.deserialize.compiled.header", || {
            cache_codec().deserialize_from(&mut cursor)
        })
        .map_err(|e| format!("Base cache compiled header decode failed: {e}"))?;
    let mut offset = cursor.position() as usize;

    let code = deserialize_section(
        bytes,
        &mut offset,
        "compiled.code",
        "cache.deserialize.compiled.code",
        "cache.section.compiled.code_bytes",
    )?;
    let functions = deserialize_section(
        bytes,
        &mut offset,
        "compiled.functions",
        "cache.deserialize.compiled.functions",
        "cache.section.compiled.functions_bytes",
    )?;
    let struct_defs = deserialize_section(
        bytes,
        &mut offset,
        "compiled.struct_defs",
        "cache.deserialize.compiled.struct_defs",
        "cache.section.compiled.struct_defs_bytes",
    )?;
    let abstract_types = deserialize_section(
        bytes,
        &mut offset,
        "compiled.abstract_types",
        "cache.deserialize.compiled.abstract_types",
        "cache.section.compiled.abstract_types_bytes",
    )?;
    let primitive_types = deserialize_section(
        bytes,
        &mut offset,
        "compiled.primitive_types",
        "cache.deserialize.compiled.primitive_types",
        "cache.section.compiled.primitive_types_bytes",
    )?;
    let show_methods = deserialize_section(
        bytes,
        &mut offset,
        "compiled.show_methods",
        "cache.deserialize.compiled.show_methods",
        "cache.section.compiled.show_methods_bytes",
    )?;
    let specializable_functions = deserialize_section(
        bytes,
        &mut offset,
        "compiled.specializable_functions",
        "cache.deserialize.compiled.specializable_functions",
        "cache.section.compiled.specializable_functions_bytes",
    )?;
    let runtime_specialization_map = deserialize_section(
        bytes,
        &mut offset,
        "compiled.runtime_specialization_map",
        "cache.deserialize.compiled.runtime_specialization_map",
        "cache.section.compiled.runtime_specialization_map_bytes",
    )?;
    let global_slot_names = deserialize_section(
        bytes,
        &mut offset,
        "compiled.global_slot_names",
        "cache.deserialize.compiled.global_slot_names",
        "cache.section.compiled.global_slot_names_bytes",
    )?;
    let global_slot_types = deserialize_section(
        bytes,
        &mut offset,
        "compiled.global_slot_types",
        "cache.deserialize.compiled.global_slot_types",
        "cache.section.compiled.global_slot_types_bytes",
    )?;
    if offset != bytes.len() {
        return Err(format!(
            "Base cache compiled payload has {} trailing bytes",
            bytes.len() - offset
        ));
    }

    Ok(CompiledProgram {
        code,
        functions,
        struct_defs,
        abstract_types,
        primitive_types,
        show_methods,
        entry: header.entry,
        specializable_functions,
        runtime_specialization_map,
        compile_context: None,
        base_function_count: header.base_function_count,
        macro_bindings: std::collections::HashMap::new(),
        global_slot_names,
        global_slot_types,
        global_slot_count: header.global_slot_count,
    })
}

fn append_section<T: Serialize>(
    bytes: &mut Vec<u8>,
    section_name: &'static str,
    value: &T,
) -> Result<(), String> {
    let payload = cache_codec()
        .serialize(value)
        .map_err(|e| format!("Base cache section {section_name} serialization failed: {e}"))?;
    append_section_bytes(bytes, section_name, &payload)
}

fn append_section_bytes(
    bytes: &mut Vec<u8>,
    section_name: &'static str,
    payload: &[u8],
) -> Result<(), String> {
    let len = u64::try_from(payload.len())
        .map_err(|_| format!("Base cache section {section_name} is too large"))?;
    bytes.extend_from_slice(&len.to_le_bytes());
    bytes.extend_from_slice(payload);
    Ok(())
}

fn deserialize_section<T: DeserializeOwned>(
    bytes: &[u8],
    offset: &mut usize,
    section_name: &'static str,
    time_label: &'static str,
    bytes_label: &'static str,
) -> Result<T, String> {
    let section = read_section(bytes, offset, section_name, bytes_label)?;
    super::profile::time(time_label, || cache_codec().deserialize(section))
        .map_err(|e| format!("Base cache section {section_name} decode failed: {e}"))
}

fn read_section<'a>(
    bytes: &'a [u8],
    offset: &mut usize,
    section_name: &'static str,
    bytes_label: &'static str,
) -> Result<&'a [u8], String> {
    let len_end = offset
        .checked_add(SECTION_LEN_BYTES)
        .ok_or_else(|| format!("Base cache section {section_name} length overflow"))?;
    if len_end > bytes.len() {
        return Err(format!(
            "Base cache section {section_name} is missing its length"
        ));
    }

    let mut len_bytes = [0u8; SECTION_LEN_BYTES];
    len_bytes.copy_from_slice(&bytes[*offset..len_end]);
    *offset = len_end;

    let len = usize::try_from(u64::from_le_bytes(len_bytes))
        .map_err(|_| format!("Base cache section {section_name} length does not fit usize"))?;
    let end = offset
        .checked_add(len)
        .ok_or_else(|| format!("Base cache section {section_name} payload overflow"))?;
    if end > bytes.len() {
        return Err(format!(
            "Base cache section {section_name} declares {len} bytes but only {} remain",
            bytes.len().saturating_sub(*offset)
        ));
    }

    let section = &bytes[*offset..end];
    *offset = end;
    super::profile::note(bytes_label, || format!("{} bytes", section.len()));
    Ok(section)
}

fn record_cache_profile(cache: &SerializedBaseCache, total_bytes: usize) {
    super::profile::note("cache.base_cache_total_bytes", || total_bytes.to_string());
    super::profile::note("cache.base_cache_counts", || {
        let method_count: usize = cache
            .method_tables
            .values()
            .map(|table| table.methods.len())
            .sum();
        let closure_binding_count: usize = cache.closure_captures.values().map(HashSet::len).sum();
        format!(
            "code={} functions={} structs={} abstracts={} primitive_types={} show_methods={} persisted_specializable={} runtime_specialization_map={} method_tables={} methods={} closure_scopes={} closure_bindings={} promotion_rules={} inference_results={}",
            cache.compiled.code.len(),
            cache.compiled.functions.len(),
            cache.compiled.struct_defs.len(),
            cache.compiled.abstract_types.len(),
            cache.compiled.primitive_types.len(),
            cache.compiled.show_methods.len(),
            cache.compiled.specializable_functions.len(),
            cache.compiled.runtime_specialization_map.len(),
            cache.method_tables.len(),
            method_count,
            cache.closure_captures.len(),
            closure_binding_count,
            cache.promotion_rules.len(),
            cache.inference_results.len()
        )
    });
}

/// Generate and serialize the Base cache in one step.
/// This is the main entry point for `--precompile-base`.
/// Triggers Base compilation, exports the cache, and serializes to bytes.
pub fn generate_base_cache() -> Result<Vec<u8>, String> {
    use crate::compile::compile_with_cache;

    // Parse a trivial program to trigger Base compilation via cache
    let program = match crate::pipeline::parse_and_lower("true") {
        Ok(p) => p,
        Err(_) => return Err("Failed to parse trivial program".to_string()),
    };

    // Compile to populate Base cache
    compile_with_cache(&program).map_err(|e| format!("Base compilation failed: {}", e))?;

    // Export cached data
    let (compiled, method_tables, closure_captures, inference_results) =
        super::cache::export_base_cache().ok_or("Base cache not populated after compilation")?;

    serialize_base_cache(
        &compiled,
        &method_tables,
        &closure_captures,
        &inference_results,
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    // ── compute_prelude_hash ───────────────────────────────────────────────────

    #[test]
    fn test_prelude_hash_is_64_hex_chars() {
        let hash = compute_prelude_hash();
        assert_eq!(
            hash.len(),
            64,
            "SHA-256 digest should be 64 hex characters, got {} chars: {}",
            hash.len(),
            hash
        );
    }

    #[test]
    fn test_prelude_hash_is_lowercase_hex() {
        let hash = compute_prelude_hash();
        assert!(
            hash.chars()
                .all(|c| c.is_ascii_hexdigit() && !c.is_ascii_uppercase()),
            "Hash should be lowercase hex, got: {}",
            hash
        );
    }

    #[test]
    fn test_prelude_hash_is_deterministic() {
        let hash1 = compute_prelude_hash();
        let hash2 = compute_prelude_hash();
        assert_eq!(hash1, hash2, "compute_prelude_hash() must be deterministic");
    }

    #[test]
    fn test_base_cache_hash_is_64_hex_chars_and_deterministic_7515() {
        let hash1 = compute_base_cache_hash();
        let hash2 = compute_base_cache_hash();
        assert_eq!(hash1, hash2, "Base cache hash must be deterministic");
        assert_eq!(
            hash1.len(),
            64,
            "SHA-256 digest should be 64 hex characters, got {} chars: {}",
            hash1.len(),
            hash1
        );
        assert!(
            hash1
                .chars()
                .all(|c| c.is_ascii_hexdigit() && !c.is_ascii_uppercase()),
            "Hash should be lowercase hex, got: {}",
            hash1
        );
        assert_ne!(
            hash1,
            compute_prelude_hash(),
            "Base cache hash must include the compiler/VM build fingerprint"
        );
    }

    #[test]
    fn test_base_cache_schema_fingerprint_is_64_hex_chars_and_deterministic_8444() {
        let fingerprint1 = base_cache_schema_fingerprint();
        let fingerprint2 = base_cache_schema_fingerprint();
        assert_eq!(
            fingerprint1, fingerprint2,
            "Base cache schema fingerprint must be deterministic"
        );
        assert_eq!(
            fingerprint1.len(),
            64,
            "SHA-256 digest should be 64 hex characters, got {} chars: {}",
            fingerprint1.len(),
            fingerprint1
        );
        assert!(
            fingerprint1
                .chars()
                .all(|c| c.is_ascii_hexdigit() && !c.is_ascii_uppercase()),
            "Fingerprint should be lowercase hex, got: {}",
            fingerprint1
        );
        assert_ne!(
            fingerprint1,
            compute_prelude_hash(),
            "Schema fingerprint must be independent of Base source text"
        );
    }

    // ── deserialize_base_cache error paths ────────────────────────────────────

    #[test]
    fn test_deserialize_empty_bytes_returns_error() {
        let result = deserialize_base_cache(&[]);
        assert!(
            result.is_err(),
            "Deserializing empty bytes should return Err"
        );
    }

    #[test]
    fn test_deserialize_garbage_bytes_returns_error() {
        let garbage = b"not a valid bincode blob at all!!!!";
        let result = deserialize_base_cache(garbage);
        assert!(
            result.is_err(),
            "Deserializing garbage bytes should return Err"
        );
    }

    // ── serialize → deserialize round-trip ────────────────────────────────────

    #[test]
    fn test_serialize_deserialize_roundtrip_empty_program() {
        use crate::vm::CompiledProgram;
        use std::collections::HashMap;

        let program = CompiledProgram {
            code: Vec::new(),
            functions: Vec::new(),
            struct_defs: Vec::new(),
            abstract_types: Vec::new(),
            primitive_types: Vec::new(),
            show_methods: Vec::new(),
            entry: 0,
            specializable_functions: Vec::new(),
            runtime_specialization_map: Vec::new(),
            compile_context: None,
            base_function_count: 0,
            macro_bindings: HashMap::new(),
            global_slot_names: Vec::new(),
            global_slot_types: Vec::new(),
            global_slot_count: 0,
        };

        let bytes = serialize_base_cache(&program, &HashMap::new(), &HashMap::new(), &[])
            .expect("serialization of empty program should succeed");
        assert!(!bytes.is_empty(), "serialized bytes must be non-empty");

        // Round-trip: version and hash both match (same process, same prelude)
        let result = deserialize_base_cache(&bytes);
        assert!(
            result.is_ok(),
            "round-trip of empty program should succeed: {:?}",
            result
        );

        let cache = result.unwrap();
        assert_eq!(
            cache.version, CACHE_VERSION,
            "deserialized version should match CACHE_VERSION"
        );
        assert!(
            cache.compiled.functions.is_empty(),
            "empty program should have no functions"
        );
        assert!(
            cache.method_tables.is_empty(),
            "empty method_tables should round-trip correctly"
        );
        assert!(
            cache.promotion_rules.is_empty(),
            "promotion_rules should be empty when registry is unpopulated"
        );
        assert!(
            cache.inference_results.is_empty(),
            "inference_results should be empty for an empty program"
        );
    }

    /// Issue #8444: a cache produced with the current `CACHE_VERSION` but an old
    /// bytecode/method-table schema must fail cleanly before its payload can be
    /// reused as if it were compatible.
    #[test]
    fn test_stale_cache_schema_fingerprint_is_rejected_8444() {
        use crate::vm::CompiledProgram;
        use std::collections::HashMap;

        let program = CompiledProgram {
            code: Vec::new(),
            functions: Vec::new(),
            struct_defs: Vec::new(),
            abstract_types: Vec::new(),
            primitive_types: Vec::new(),
            show_methods: Vec::new(),
            entry: 0,
            specializable_functions: Vec::new(),
            runtime_specialization_map: Vec::new(),
            compile_context: None,
            base_function_count: 0,
            macro_bindings: HashMap::new(),
            global_slot_names: Vec::new(),
            global_slot_types: Vec::new(),
            global_slot_count: 0,
        };

        let bytes = serialize_base_cache(&program, &HashMap::new(), &HashMap::new(), &[])
            .expect("serialization of empty program should succeed");
        let mut cursor = std::io::Cursor::new(bytes.as_slice());
        let mut header: CacheEnvelopeHeader = cache_codec()
            .deserialize_from(&mut cursor)
            .expect("cache header should decode");
        header.schema_fingerprint =
            "0000000000000000000000000000000000000000000000000000000000000000".to_string();

        let mut stale_bytes = cache_codec()
            .serialize(&header)
            .expect("rewritten header should encode");
        stale_bytes.extend_from_slice(&bytes[cursor.position() as usize..]);

        let err_msg = deserialize_base_cache(&stale_bytes)
            .expect_err("mismatched schema fingerprint must reject the cache");
        assert!(
            err_msg.contains("schema fingerprint mismatch"),
            "expected schema-fingerprint rejection, got: {}",
            err_msg
        );
    }

    // ── version mismatch detection ─────────────────────────────────────────────

    #[test]
    fn test_version_mismatch_returns_error() {
        use crate::vm::CompiledProgram;
        use std::collections::HashMap;

        // Build a cache with a wrong version number
        let wrong_version_cache = SerializedBaseCache {
            version: CACHE_VERSION + 1,
            source_hash: compute_prelude_hash(),
            compiled: CompiledProgram {
                code: Vec::new(),
                functions: Vec::new(),
                struct_defs: Vec::new(),
                abstract_types: Vec::new(),
                primitive_types: Vec::new(),
                show_methods: Vec::new(),
                entry: 0,
                specializable_functions: Vec::new(),
                runtime_specialization_map: Vec::new(),
                compile_context: None,
                base_function_count: 0,
                macro_bindings: HashMap::new(),
                global_slot_names: Vec::new(),
                global_slot_types: Vec::new(),
                global_slot_count: 0,
            },
            method_tables: HashMap::new(),
            closure_captures: HashMap::new(),
            promotion_rules: Vec::new(),
            inference_results: Vec::new(),
        };

        let bytes = cache_codec()
            .serialize(&wrong_version_cache)
            .expect("serialization should succeed even with wrong version");

        let result = deserialize_base_cache(&bytes);
        assert!(result.is_err(), "wrong version should return Err");

        let err_msg = result.unwrap_err();
        assert!(
            err_msg.contains("version mismatch"),
            "Error message should mention 'version mismatch': {}",
            err_msg
        );
    }

    /// Issue #5968: a snapshot from an older `CACHE_VERSION` whose later fields
    /// changed layout (e.g. the `CachedReturn.method_edges` field appended in
    /// #5967) must be rejected at the **version gate**, BEFORE the full
    /// positional bincode decode is attempted. Otherwise the now-incompatible
    /// payload misaligns and surfaces as a cryptic "Deserialization failed" — or
    /// worse, silently misdecodes. We don't need to reconstruct the whole old
    /// struct: a valid version-prefix at the previous version followed by an
    /// arbitrary (incompatible) payload must be turned away with a clear
    /// version-mismatch error and the payload never decoded.
    #[test]
    fn test_old_layout_snapshot_rejected_by_version_gate_5968() {
        // A leading u32 == CACHE_VERSION - 1 (bincode encodes `version` first),
        // then bytes that do NOT form a valid current SerializedBaseCache.
        let mut bytes = cache_codec()
            .serialize(&CacheVersionHeader {
                version: CACHE_VERSION - 1,
            })
            .expect("header serialization should succeed");
        bytes.extend_from_slice(b"old-layout payload that must never be decoded");

        let result = deserialize_base_cache(&bytes);
        let err_msg = result.expect_err("an older-version snapshot must be rejected");
        assert!(
            err_msg.contains("version mismatch"),
            "expected a clean version-mismatch rejection (not a decode error), got: {}",
            err_msg
        );
    }

    /// Issue #5968: the version gate reads only the leading prefix, so it must
    /// reject an old-version snapshot even when the trailing bytes are too short
    /// to form a full struct — proving the gate runs before (and independently
    /// of) the full decode.
    #[test]
    fn test_version_gate_runs_before_full_decode_5968() {
        let bytes = cache_codec()
            .serialize(&CacheVersionHeader {
                version: CACHE_VERSION - 1,
            })
            .expect("header serialization should succeed");
        // No trailing payload at all: a full SerializedBaseCache decode would
        // fail with a truncation error, but the version gate fires first.
        let err_msg =
            deserialize_base_cache(&bytes).expect_err("older version must be rejected at the gate");
        assert!(
            err_msg.contains("version mismatch"),
            "expected version-mismatch from the gate, got: {}",
            err_msg
        );
    }
}
