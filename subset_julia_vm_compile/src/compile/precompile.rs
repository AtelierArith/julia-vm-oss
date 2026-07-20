//! Precompiled Base cache serialization.
//!
//! Provides save/load for the `SerializedBaseCache`, which contains
//! all data needed to skip Base compilation at startup.

// Issue #10906 (Phase 1c of #10869): the Base cache serialize/deserialize
// boundary named in Issue #10869 — zero real unwrap_used/expect_used/panic
// sites in production code (every match is inside cfg(test) items, which
// carry an explicit allow). `deserialize_base_cache` already returns
// `Result` and treats a load failure as a cache miss
// (`docs/vm/CACHE_ARCHITECTURE.md`).
#![deny(clippy::unwrap_used)]
#![deny(clippy::expect_used)]

use serde::{de::DeserializeOwned, Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::collections::{HashMap, HashSet};

#[cfg(test)]
use crate::bytecode::SpecializationDisableFlags;
use crate::bytecode::{CompiledProgram, Instr};

use super::abstract_interp::engine::{CachedReturn, InferenceCacheKey};
use super::{MethodTable, MethodTableKey};

/// Version of the cache format. Increment on breaking changes.
///
/// Bumped to 70 for Issue #8626: the cache envelope header gained the
/// `enum_variant_fingerprint` field — a hash of the variant-name lists (in
/// declaration order) of the wire-format enums `Instr` / `BuiltinId` /
/// `Intrinsic` / `BuiltinOp`. bincode encodes enums positionally by variant
/// index, so inserting/removing/reordering variants silently re-tags every
/// later variant in older caches; the fingerprint turns that silent corruption
/// into detection + automatic regeneration from source.
///
/// Bumped to 69 for Issue #8544: `LatticeType` gained the tail-appended
/// `PartialStruct` variant, which is part of the persisted wire format via
/// `inference_results` (`InferenceCacheKey` argtypes and `CachedReturn.ty`).
/// The append keeps older snapshots decodable by the new code, but newer
/// snapshots can carry the new bincode tag (and constructor-site inference
/// results changed shape from `Concrete(Struct)` to `PartialStruct`), so
/// caches built by older compilers must be regenerated.
/// `src/compile/lattice/types.rs` is now a schema-manifest input (#8444).
///
/// Bumped to 68 for Issue #8545: flow-sensitive early-return narrowing and
/// generalized predicate inlining (InterConditional) change the inference
/// results snapshot into cached method signatures (e.g. guarded
/// `Union{T,Nothing}` functions now infer `T` returns), so Base caches built
/// by older compilers must be regenerated. No serialized data shape changed,
/// but `engine/mod.rs` is a schema-manifest input (#8444) and the semantic
/// shift alone warrants invalidation.
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
// Bumped to 72 for Issue #8713: `CompiledProgram` gained an instruction-indexed
// source_map field used by runtime stack trace diagnostics.
// Bumped to 73 for Issue #8656: bytecode-owned `BuiltinId`/`Intrinsic` and
// their stable wire-id tables moved into `subset_julia_vm_bytecode`.
// Bumped to 74 for Issue #8656: bytecode-owned `ArrayElementType`/`ValueType`
// moved into `subset_julia_vm_bytecode`, changing cache schema ownership.
// Bumped to 75 for Issue #8656: bytecode-owned instruction operand payloads
// moved into `subset_julia_vm_bytecode`, changing cache schema ownership.
// Bumped to 76 for Issue #8656: `MakeGenerator` now serializes a bytecode-owned
// callable spec instead of the VM runtime callable enum.
// Bumped to 77 for Issue #8656: `PushArrayValue` now serializes bytecode-owned
// literal array payloads instead of VM `ArrayValue`.
// Bumped to 78 for Issue #8656: `Instr` moved into `subset_julia_vm_bytecode`.
// Bumped to 79 for Issue #8656: struct/abstract/primitive/show metadata moved
// into `subset_julia_vm_bytecode`.
// Bumped to 80 for Issue #8656: slot type metadata moved into
// `subset_julia_vm_bytecode`.
// Bumped to 81 for Issue #8656: runtime-specialization program metadata moved
// into `subset_julia_vm_bytecode`.
// Bumped to 82 for Issue #8656: the Value model (vm/value), VmError, rng,
// slot info, peephole, program containers (vm/types.rs -> program.rs), and
// StructInfo moved into `subset_julia_vm_bytecode`.
// Bumped to 83 for Issue #9104: `SpecializableFunction.ir` wrapped in `Arc`.
// The bincode wire format is unchanged (serde serializes `Arc<T>` as `T`),
// but the schema-input source changed, so version and fingerprint move
// together per the audit policy.
// Bumped to 84 for Issue #9112: `MethodTable` gained the `#[serde(skip)]`
// `first_arg_index` field (lazy dispatch pre-filter) and `CorePrimitive`
// gained `julia_name()`. The bincode wire format is unchanged (skipped field,
// method-only addition), but both files are schema inputs, so version and
// fingerprint move together per the audit policy.
// Bumped to 85 for Issue #9140: `CompiledProgram.functions` wrapped in
// `Vec<Rc<FunctionInfo>>` so compiles share the cached Base entries instead
// of deep-cloning them. Wire format unchanged (serde serializes `Rc<T>` as
// `T`); schema-input source changed, so version and fingerprint move together.
// Bumped to 86 for Issue #9126: two F64 accumulate superinstructions
// (`Instr::AddF64Slots` / `AddF64I64Slots`) appended to the `Instr` enum.
// Appending preserves existing bincode declaration-order wire IDs but changes
// `enum_variant_fingerprint`, so version and fingerprint move together.
// Bumped to 87 for Issue #9198 S4: `ArrayElementType::StructInlineF64` appended
// (last, after `F16`) for byte-contiguous all-`Float64` isbits-struct array
// storage. `ArrayElementType` is embedded in serialized `Instr` payloads and is
// a schema-manifest input (`base_cache_schema_files.txt`), so appending changes
// the schema fingerprint; the bump forces a clean rebuild of any pre-change
// cache. (It is NOT part of `enum_variant_fingerprint`, which covers only
// `Instr`/`BuiltinId`/`Intrinsic`/`BuiltinOp`.) The companion
// `ArrayData::StructF64` is never bincode-serialized, so it has no wire impact.
// Issue #9197 S7: bumped 87 → 88 when the Base-cache method-table map key was
// demoted from a bare `String` to the typed [`MethodTableKey`] and its section
// began serializing in sorted (deterministic, hash-seed-independent) key order.
// Bumped to 89 for Issue #9377: `Instr::CoerceRangeStopI64`'s stack contract
// gained a third `start` operand (top → down: `stop`, `step`, `start`) so the
// handler can distinguish the legal empty direction from a counting-direction
// InexactError. The bincode wire format is unchanged (unit variant), but
// bytecode compiled by the old codegen pushes only `step`+`stop`, which the new
// handler would misread — the bump forces cached Base bytecode to regenerate.
// Bumped to 90 for Issue #9519: `BuiltinOp::RangeStep` was appended and native
// range dispatch identity now includes the explicit `StepRange{T,S}` step type.
// Regenerate cached Base bytecode/method tables so `step(::StepRange)` routes
// through `_range_step` and dispatch caches keep distinct `S` parameters.
// Bumped to 91 for Issue #9090: `InferenceCacheKey` / `CacheArgType` moved
// from the compile engine into `subset_julia_vm_types::inference_cache_key`.
// The bincode shape is unchanged, but the schema source of truth moved below
// the compile crate boundary, so cached Base snapshots are regenerated under
// the new boundary.
// Bumped to 92 for Issue #9090/#10090: `MethodTable`/`MethodSig` moved from
// `subset_julia_vm::runtime_types::method_table` into
// `subset_julia_vm_bytecode::method_table` (commit f003e2964) without this
// version bump, so the audit snapshot silently drifted from the real
// `SJULIA_BASE_CACHE_SCHEMA_HASH` (Issue #10090, same failure mode as
// #9498). The bincode shape is unchanged; the schema-input file moved
// crates, so version and fingerprint move together per the audit policy.
// Bumped to 93 for Issue #9803 (merge renumber: both this and the #10090
// bump above landed as 92 on their own branches; 92 shipped first, so the
// #9803 Expr change takes 93): `subset_julia_vm_types::ir::core::Expr`
// gained a new `Convert` variant (structural numeric type-constructor node,
// carried inside cached `SharedFunctionPlan`s). `Expr` is plain-derive
// serialized by declaration order and is not covered by
// `enum_variant_fingerprint` (that only tracks `Instr`/`BuiltinId`/
// `Intrinsic`/`BuiltinOp`), so a stale on-disk cache built against the old
// `Expr` shape must be rejected rather than misdecoded.
//
// Bumped to 94 for Issue #10118 (merge renumber: this and the #9803 bump
// above both landed as 93 on their own branches; 93 shipped first via the
// milestone #69 merge order, so the #10118 codec change takes 94): every
// section payload (and the envelope / compiled-program headers) is now
// encoded with `postcard` instead of bincode's varint mode (`cache_codec`,
// removed). Both are plain serde-derive-based codecs over the SAME
// `Serialize`/`Deserialize` structs (no `#[derive(Archive)]`/layout changes
// needed, unlike a zero-copy format such as rkyv), so this is a
// wire-encoding swap only — measured ~3-5x faster decode with a smaller
// payload (see `docs/vm/STATUS.md` 2026-07-10 entry for A/B numbers). The
// version bump forces every existing persistent/embedded cache
// (bincode-encoded) to be rejected and regenerated rather than misdecoded:
// an old cache's leading bytes are read by the NEW postcard
// `CacheVersionHeader` decode, and even in the practically-impossible case
// that a stale bincode-varint u32 happens to decode as SOME postcard value,
// it would need to additionally match `CACHE_VERSION`, the magic bytes, the
// schema fingerprint, the compiler build fingerprint, AND the source hash for
// `deserialize_base_cache` to accept it — see `postcard_rejects_stale_bincode_cache_10118`.
//
// Bumped to 95 for Issue #10093 (merge renumber: this and the #10118 bump
// above both landed as 94 on their own branches; 94 shipped first, so this
// takes 95): `BuiltinId`/`BuiltinOp` gained the tail-appended
// `TestRecordError` variant (`_test_record_error!`, the errored `@test`
// outcome recorder). Both enums are covered by `enum_variant_fingerprint`, so
// old caches would already be rejected at load time; the version bump keeps
// the human-readable changelog and the audited schema fingerprint moving
// together per the audit policy.
//
// Bumped to 98 for Issue #10107: `Instr` gained the tail-appended `TakeSlot`
// variant (destructive slot load with move semantics), stacked on top of the
// concurrent bump to 97 on main. `Instr` is covered by
// `enum_variant_fingerprint`, so old caches would already be rejected at load
// time; the version bump keeps the human-readable changelog and the audited
// schema fingerprint moving together per the audit policy.
// 100: stack bytecode gained TakeSlot and JumpIfCmpI64SlotConst (Issues
// #10107/#10105), changing serialized Instr discriminants.
// 101: `Instr` gained the tail-appended `CallSpecializeF64Slots` /
// `CallSpecializeInboundsF64Slots` variants (Issue #10491, the Float64 mirror
// of the I64 slot-fused specialize calls). `Instr` is covered by
// `enum_variant_fingerprint`, so old caches would already be rejected at load
// time; the version bump keeps the human-readable changelog and the audited
// schema fingerprint moving together per the audit policy.
// 102: statement IR gained explicit literal-tuple destructuring identity
// (Issue #10444), changing the compiler schema fingerprint.
// 103: stack bytecode gained the tail-appended ApplyTypeDynamicSplat variant
// (Issue #10191), stacked on top of the #10444 bump and changing the serialized
// Instr schema again.
// 104: another branch landed CACHE_VERSION 104 concurrently.
// 105: SpecializedCode gained local_slot_count for runtime-specialized
// ComplexF64 SROA bodies (Issue #10567).
// 106: `RuntimeTypeVarValue::projection()` preserves an anonymous
// contravariant TypeVar's lower bound (Issue #10373).
// 107: `AbstractTypeDefInfo` now serializes complete `TypeParam` declarations
// instead of names alone, preserving upper/lower bounds for runtime abstract
// wrapper construction and `Core.apply_type` validation (Issue #10554).
// 108: `JuliaType` gained identity-bearing `RuntimeTypeVar` and structured
// `RuntimeParametric` variants so runtime-created TypeVars survive type
// application and reflection without collapsing distinct objects that share a
// name and bounds; its structured projection subsumes the #10373 string
// encoding while preserving both bounds exactly (Issues #10554, #10613).
// 109: `JuliaType::RuntimeUnionAll` preserves runtime binder identity through
// nested same-name UnionAll construction, reflection, and exact-id
// instantiation instead of collapsing binders by their rendered names (Issue
// #10613).
// 110: runtime UnionAll alpha aliases reserve every original binder name before
// generating suffixes, preventing a generated alias from colliding with a
// later original binder and corrupting subtype recursion (Issue #10613).
// 111: `CoreTypeVar` gained explicit rigid runtime identity so semantic type
// equality, subtyping, and typejoin preserve free RuntimeTypeVar objects while
// alpha-normalizing enclosing RuntimeUnionAll binders (Issue #10613).
// 112: source anonymous-bound shorthand uses an internal TypeVar marker until
// parametric construction, preserving runtime bound validation (#10373) while
// keeping explicit `TypeVar(:_)` values identity-bearing (#10613).
// 113: statement IR gained a flat non-literal destructuring assignment shape
// consumed structurally by the VM, specialization, and AoT pipelines (Issue
// #10464).
// 114: catch up the cache version with the audited schema fingerprint after a
// schema-sensitive mainline change landed with a stale snapshot (Issue #10686).
// 115: `CoreTypeVar` exposes a structured identity projection for matching
// keys, separating unresolved, scoped, and rigid runtime binders (Issue
// #10459).
// 116: `RegexMatchValue` gained a `capture_names` field (parallel to
// `captures`) so named-group access (`m[:name]` / `keys(m)` / `haskey`) works,
// changing the serialized `Value` schema (Issues #10173, #10182).
// 118: `Expr::Call.function` uses the IR string interner while preserving its
// string wire payload semantics (Issue #10184).
// 119: Remaining identifier-like `Expr` fields use the IR string interner while
// preserving their string wire payload semantics (Issue #10184).
// 120: runtime-dispatched `Intrinsic` variants renamed `AddFloat`/`SubFloat`/
// `MulFloat`/`DivFloat`/`PowFloat` -> `DynamicAdd`/`DynamicSub`/`DynamicMul`/
// `DynamicDiv`/`DynamicPow` (Issue #10754; merge renumber: this and the #10184
// bump above both landed as 119 on their own branches; #10184 shipped first, so
// this takes 120). Stable wire IDs (8-12) and the `add_float`/... `from_name`
// strings are unchanged, so the serialized shape is byte-identical; `Intrinsic`
// is covered by `enum_variant_fingerprint`, so old caches would already be
// rejected at load time. The version bump keeps the human-readable changelog and
// the audited schema fingerprint moving together per the audit policy.
// 121: `value_type_for_struct_instance` maps MemoryRef-backed `Array{T,N}`
// wrappers to `ValueType::ArrayOf` so the runtime specializer accepts
// struct-Vector arguments (Issue #10566 blocker (a); merge renumber: this
// landed as 119 on its own branch, but #10184 and #10754 shipped 119/120
// first).
// 124: the reflection fixture manifest now covers call-site re-inference of an
// exact overloaded method body when its static type-value return snapshot
// widens to `Top` (Issue #10133). The manifest is schema-fingerprinted so Base
// caches and their acceptance-test inventory advance together.
// 125: array-constructor fixture metadata now records the completed
// direct/callable parity closure and its companion/metamorphic gates (Issue
// #10250). Fixture manifests are part of the audited Base-cache schema input.
// 126: the dependent-bound fixture now requires owner-scoped TypeVar object
// identity and exact cross-wrapper typejoin precision (Issue #10261).
// 127: Issue #10349 adds six serialized BuiltinId wire variants for VM Task
// continuation boundaries.
// 128: no serialized wire shape changed — this file gained a
// `#![deny(clippy::unwrap_used, clippy::expect_used)]` pragma, an
// explanatory comment, and cfg(test)-item allow attributes for the
// cache-load-boundary zero-deny gate (Issue #10906, Phase 1c of #10869;
// merge renumber: this landed as 127 on its own branch, but #10349 shipped
// 127 first). Bumped only because `precompile.rs` is a schema-manifest
// input (`base_cache_schema_files.txt`) and any edit to it changes the
// audited schema fingerprint by design, per
// `scripts/audit_base_cache_schema_fingerprint.sh`'s policy.
// 129: no serialized wire shape changed — `compile/abstract_interp/engine/mod.rs`
// (also a schema-manifest input) had its two raw-unwrap panic-debt sites
// converted to typed control flow: `record_backedge_module_call` now binds
// `active_specialization` once via `let Some(..) = .. else { return }`
// instead of an `is_none()` check followed by a later re-read, and the three
// `lower_block_to_cfg` call sites route through its new `Option`-returning
// signature (Issue #10905, Phase 1b of #10869). Bumped only because the file
// is a schema-manifest input and any edit to it changes the audited schema
// fingerprint by design, per `scripts/audit_base_cache_schema_fingerprint.sh`'s
// policy.
// 130: `InnerConstructor` records whether its implicit constructor self is
// explicit-parametric, and `StaticParametricCall` records whether caller type
// bindings must be forwarded. Both fields use serde defaults for source-level
// compatibility, but they change serialized cache input/wire shapes (Issues
// #10959 and #10967).
// 131: no wire shape changed — `compile/instr_wire_ids.rs` (a schema-manifest
// input) gained a cfg(test) clippy allow pragma (Issue #10908 Phase 3 of
// #10869; merge renumber: this landed as 130 on its own branch, but
// #10959/#10967 shipped 130 first).
// 132: `MethodTable` gains a serialized, deterministic
// `BTreeMap<usize, ConstructorSelfFamily>` origin carrier
// (`constructor_self_families`, serde-defaulted). Constructor-self identity
// (bare `Type{Foo}` vs explicit `Type{Foo{T}}`) is now a required, typed,
// cache-surviving part of method-table identity instead of a transient
// `SharedCompileContext` set that never reached the Base cache (Issue
// #10962, #10974).
// 133: no wire shape changed — schema-manifest inputs abstract_interp/engine/mod.rs and this file gained the for/foreach lexical-scope shadow/restore edits (with siblings in inference/stmt/core_compiler/expr/collection, Issue #10984 / #10903).
// (merge renumber: landed as 132 on its own branch, but #10962/#10974
// shipped 132 first.)
// 134: `StaticParametricCall` gains serde-defaulted runtime argument-validation
// metadata plus an optional candidate-specific fallback function/binding
// payload. The live-append gate validates both serialized function indices
// before accepting a cached delta (Issues #10969, #10993).
// 135: source-definition spans carry serialized Julia evaluation ordinals.
// The compiler uses those ordinals, rather than file-local byte
// offsets, to resolve identical bare inner/ordinary constructor definitions
// across `include` boundaries (Issue #11028).
// 137: `CompiledProgram::macro_bindings` re-keyed from a bare module-path
// `String` to the new interned `ModuleId` (`subset_julia_vm_bytecode`), plus a
// new `module_registry: ModuleInternTable` field (the path <-> id relocation
// table `macro_bindings` resolves through) — Issue #10988 Phase 2a's
// cache-relocation-pattern deliverable. This IS a genuine wire-shape change
// for the whole-struct bincode boundaries (`.sjvmbc`/the prelude Program
// cache: a pre-#10988 payload has no `ModuleId` keys and cannot be
// reinterpreted, so the version bump invalidates it rather than partially
// decoding — `docs/vm/CACHE_ARCHITECTURE.md`'s invalidate-on-mismatch
// contract). The persistent/embedded Base cache's own section format never
// serialized `macro_bindings` at all (`append_compiled_program_section`/
// `deserialize_compiled_program_body` always reset it to empty — it is
// rebuilt fresh by the compile pipeline for whatever program reuses the Base
// prefix), so THIS bump is, for that specific format, the same
// "no wire shape changed but a schema-manifest input's content did" case as
// 128/129/131/133 above.
// (merge renumber: this landed as 134 on its own branch, but #10969/#10993
// (134) and #11028 (135, const 136) shipped first.)
// 138: no wire shape changed — schema-manifest input abstract_interp/engine/mod.rs
// (and this file) changed by the mechanical clippy 0.1.97 lint fixes
// (redundant `filter: _` field patterns, map-values iteration) shipped with
// the Milestone-73 batch (Issues #10977/#10980 and siblings). Merge renumber:
// this landed as 134/135 on its own branches, but #10969/#10993 shipped 134
// and #11028 shipped 135/136 first.
// 139: `KwParamInfo` gains a serde-defaulted `declared_type: Option<JuliaType>`
// carrying a keyword parameter's DECLARED type to the bind site, where it is
// asserted against the supplied value (upstream `TypeError: in keyword argument
// x, expected Int64, got a value of type Float64`). `ty` is the lossy slot
// `ValueType` and cannot express an abstract annotation such as `x::Real`
// (Issues #11024, #11081).
// 140: constructor-self dedup reconstructs the implicit `Type{Foo{...}}`
// signature so self-only binder bounds remain part of method identity, while
// registration expands user aliases, qualifies module-local self arguments,
// and validates nested dependent bounds. `MethodSig` retains the selected
// callable-self pattern but no duplicate origin booleans;
// `MethodTable::constructor_self_families` is the sole origin authority
// (Issues #11019, #11043, and #11062).
// 141: merge integration combines the independently developed version-139
// keyword-annotation wire change with the version-140 constructor-identity
// wire change. Advancing again prevents either branch's cache from being read
// as the combined schema.
// 142: merge integration adds owner-preserving CoreType projections for
// qualified user structs to the version-141 schema. Although that dispatch
// projection does not add serialized fields, its cache semantics changed;
// invalidate older caches so bare projections cannot mix with owner-aware
// runtime arguments (#11076).
// 143: `RuntimeCompileContext::struct_table` becomes an owner-scoped
// `StructRegistry` (`StructId { module: ModuleId, local }`-keyed entries plus a
// name -> id alias index) instead of a bare-name `StructInfo` map (#11078).
// The BINCODE WIRE IS UNCHANGED — that field is `#[serde(skip)]` (#3973) and
// the ids are DERIVED on both lanes, never persisted or relocated
// (`docs/vm/CACHE_ARCHITECTURE.md` Pattern A) — but a schema file on the
// fingerprint list changed and the cache-restore path now rebuilds the struct
// table through the registry, so invalidate older caches rather than reason
// about mixing restore lanes.
// 144: no wire shape changed — schema-manifest inputs compile/inference.rs,
// compile/abstract_interp/engine/mod.rs, compile/stmt.rs, compile/context.rs and
// compile/pipeline_ctx.rs changed: the `catch` variable now binds as `Any` in the
// slot-typing pre-scan and the return-type engine (Issue #10999), and functions
// defined in a module-level `let` get their captures pre-analyzed with the global
// method's closure bound to the module name (Issue #11015).
// 145: wire shape CHANGED. `StaticParametricCall.runtime_binding_names`
// (runtime type-argument binders for `Foo{typeof(x)}(x)` inner-constructor
// calls, Issue #10998), `Function.new_struct_name` and
// `StructDef.global_new_helpers` (struct-body `global` helpers with privileged
// `new`, Issue #11005). All three are `#[serde(default)]`, but a cache written
// before them would silently drop the inner-constructor routing and the helper
// methods, so old entries must not be reused.
// 146: `Instr` gains a new variant, `ThrowUndefVarError(String)` (appended at
// the end of the enum, mirroring `ThrowMethodError`'s shape), so a call to a
// name that resolves to no function/method/builtin anywhere raises the
// upstream-matching `UndefVarError` instead of a generic `ErrorException`
// (Issue #10354's `@test_throws` type-check fix). `Instr` is serialized by
// declaration order (Issue #8627's wire-ID table is deferred for this large
// enum), so a new variant changes `enum_variant_fingerprint` and any
// already-serialized Base cache containing a call to an undefined name
// becomes unreadable; bump forces a rebuild.
// (merge renumber: this landed as 143, then 144, on its own branch, but main's
// own 143/144/145 shipped first each time.)
// 147: `ArrayElementType::Structured(Box<JuliaType>)` is appended to preserve
// identity-bearing UnionAll/RuntimeTypeVar element types without rendering and
// reparsing them through `Abstract(String)` (Issue #11236). ArrayElementType is
// embedded in serialized instruction payloads, so the new discriminant and
// payload require stale Base/prelude caches to be regenerated.
// 148: `using`/`import` spans now consume definition ordinals and package
// Modules are inserted at those source anchors. Base/prelude constructor
// chronology compiled from a pre-#11036 Program is therefore stale even
// though the Span wire shape itself was introduced earlier (#11028/#11128).
// 149: `RuntimeCompileContext` now serializes `module_imported_bindings`, the
// destination-qualified live-import alias map used by runtime Module
// reflection (Issue #11176). Reusing a cache without it would silently lose
// imported-binding provenance, so force existing Base caches to rebuild.
// 150: no wire shape changed — the schema-manifest input
// `subset_julia_vm_bytecode/src/method_table.rs` now routes compile-time Core
// signature matching through the user `StructHierarchy`, so structured
// `Type{W{T}}` lower and upper bounds participate in cached dispatch decisions
// (Issue #11233). Invalidate older Base caches whose precompiled call choices
// were made without those hierarchy-aware bound checks.
// 151: `Stmt::LocalDecl` is appended to the serde-derived Core IR statement
// enum as a typed Core.NewvarNode analogue (Issue #11281). Persisted prelude
// Programs therefore gain a new bincode discriminant; invalidate older caches
// instead of allowing declaration provenance to disappear on reuse.
// 153: `Value::CharMalformed`, its bytecode/lowering discriminants, and exact
// invalid-UTF-8 iteration change serialized cache payloads (Issue #8995).
// 154: no wire shape changed — `MethodTable` gains transient, serde-skipped
// owner provenance for source-synthesized default constructors. Invalidate the
// prior source fingerprint so cache acceptance stays explicit (Issue #11147).
// 155: `RuntimeTypeNameValue` adds nominal owner identity in schema-tracked
// `Value` source; conservatively invalidate prior caches (#8451).
// 156: `ModuleOperands` carries Base-export visibility for ordinary versus
// bare modules, changing serialized PushModule payloads (Issue #11410).
// 157: schema-tracked builtin/type registries gain reflection-only Core/Base
// namespace authority; conservatively invalidate prior caches (#11410).
// 158: `ModuleOperands` separates implicit `eval`/`include` provenance from
// Base-export visibility, changing serialized PushModule payloads (#11410).
// 159: `BuiltinId` gains `SteprangelenF64` (wire ID 316) for the
// TwicePrecision `range(start; step, length)` form (Issue #9509);
// conservatively invalidate prior caches.
// 160: compile-time struct lookup now derives declaration ownership from
// `StructRegistry`'s owner-scoped index instead of the duplicated bare-name
// fallback table (Issue #11046). Invalidate precompiled call choices made with
// the retired name-based authority.
// 161: no wire shape changed — schema-tracked `method_table.rs` gains the
// all-registration-order concrete `Complex` dispatch regression for Issues
// #10775 / #11492. Refresh the source fingerprint so cache acceptance stays
// explicit.
// 162: `CompiledProgram` persists the deterministic top-level inference-global
// type snapshot required to rebuild runtime reflection state exactly on cache
// restore (Issue #10333).
// 163: `BuiltinId::_ModuleName` added (nameof(::Module) reflection,
// Issue #11171) — new builtin wire ID invalidates serialized programs.
// 164: runtime parametric-constructor dispatch (Issues #10968/#10971)
// changes instruction operand shapes in `instr.rs`/`operands.rs` (explicit
// per-candidate type-argument binding), invalidating serialized programs.
// 165: `BuiltinId` gains `ThrowMethodErrorWithArgs` (wire ID 318) and the
// compile-time dispatch-miss sites emit it instead of Pop+ThrowMethodError so
// a caught MethodError carries upstream's `.f`/`.args` payload (Issue
// #11374); conservatively invalidate prior caches.
// 166: `CompiledProgram` persists the finalized specialization-disable flags
// computed from resolved fresh method tables. Cache restore now replays those
// decisions exactly instead of rescanning partial source spellings (#10334).
// 167: `Instr::DefineEvalStruct(usize)` records source-ordered concrete-struct
// activation in serialized bytecode (Issues #9784/#11546). Its new enum
// discriminant changes the derived `Instr` wire shape, so stale Base/prelude
// caches must be rebuilt.
// 168: `RegexMatchValue` gains a `regex: RegexValue` physical field (Issue
// #11382) so RegexMatch field projection matches upstream's 5-field layout.
// The new field changes the serialized `Value` shape (value_enum.rs is
// schema-tracked), so stale caches must be rebuilt.
// 169: structured UnionAll/TypeVar semantics now participate in canonical
// method signatures, subtype/equality decisions, and runtime type application
// through `MethodSig`'s canonical `core_signature` alongside its compatibility
// projection (Issue #10460). The schema-tracked method-table wire shape changes,
// and cached inference built with the previous name-based projections is stale.
// 170: `Instr` appends the explicit root lexical-environment operations used by
// hard-scope live REPL transactions (Issues #11569/#9784). The new enum
// discriminants change serialized programs, so stale caches must be rebuilt.
// 171: `ReplDefinitionActivation` gains an atomic function/caller-refresh group
// used by live method mutation transactions (Issue #9784). The new serialized
// enum variant changes cached program metadata, so stale caches must be rebuilt.
// 172: `Instr::CallDynamic` replaces its anonymous tuple with boxed
// `DynamicCallOperands`, retaining the compiler-resolved callee name for the
// shared semantic resolver (Issue #10461). The serialized operand shape changes,
// so stale Base/prelude caches must be rebuilt.
// 173: `Instr` appends abstract/primitive source-publication markers,
// `ReplDefinitionActivation` gains abstract/primitive/enum variants, and
// `CompiledProgram` persists source-ordered `enum_defs` as a new cache section
// for live nominal-type transactions (Issues #9784/#11635).
// 174: `RegisterEnumOperands` records the exact member-binding subset that a
// recovered partial enum may replay without reviving failed/unreached constant
// stores (Issue #11652). The serialized operand shape changes.
// 175: `Instr::RaiseUndefVarErrorIfFunctionInvisible(String)` (Issue #11320)
// is a new enum discriminant, changing the derived `Instr` wire shape, so
// stale Base/prelude caches must be rebuilt.
// 176: `Instr::DefineRuntimeNominal` and its structured four-family payload are
// appended for runtime-conditional type publication (Issue #11654). The new
// instruction discriminant and operand metadata change serialized bytecode.
// 177: `ReplDefinitionActivation::RuntimeNominal` records the observed registry
// identity and enum member prefix for conditional type transactions (#11654).
// 178: runtime struct operands retain the complete source `StructDef` beside
// their concrete layout so parameter and constructor semantics are not erased
// at the compiler/VM boundary (Issues #11678/#11679).
// 179: `Instr::ProbeRuntimeBinding` separates source-order signature probes
// from eager compiler-owned constructor/type metadata (Issues #11025/#11654).
// 180: runtime-nominal recovery now persists main-inline method snapshots and
// pre-optimization module inventories; rebuild cached compiler metadata so no
// stale snapshot can bypass the hardened publication checks (#11654).
// 181: `Program::definition_order_bounds` is now the public cross-crate
// chronology authority used by runtime-nominal activation selection (#11654).
// 182: `FunctionInfo` records whether a compiled function is a private lowering
// helper so live REPL installation and callable rebasing cannot expose helpers
// as Julia-visible methods (Issue #9784). Rebuild cached programs against the
// new serialized runtime metadata contract.
// 183: `Instr::CreateResolvedClosure` freezes a closure's callable candidates,
// and `FunctionInfo` retains source definition order so reflection follows the
// same replacement chronology as runtime dispatch (Issue #9784).
// 184: function and closure runtime values retain a stable callable singleton
// identity (declaration owners plus source/lowering-helper provenance) instead
// of using relocated candidate indices as dispatch-cache identity (#11685).
// 185: callable-struct bound-self is now marked structurally at lowering time
// (Issues #11386/#11553) — the synthesized `self` parameter name carries a
// reserved marker prefix that `FunctionInfo::callable_binds_self()` reads. An
// old cached program has no marker, so its bound callables would misclassify;
// invalidate serialized programs.
// 186: `Instr::ActivateUsing` records the owner-scoped source identity of each
// completed using/import statement for exact REPL error recovery (#11748).
// The new instruction discriminant and operands change serialized bytecode.
// 187: `Instr::ActivateModule` records each owner-qualified module body whose
// binding was published before execution for exact failed-module recovery
// (#11761). The new instruction discriminant changes serialized bytecode.
// 188: typed-array-literal element metadata moved into the shared bytecode
// crate so the compiler and runtime specializer use one serialized schema
// (#10746).
// 189: runtime-nominal operands and activations record same-input root
// coalescing so repeated sites reuse one registry identity (#11684). The
// serialized operand and recovery metadata shapes change.
// 190: every typed-array-literal element now emits `convert(T, x)` before
// `MemorySet`; cached programs compiled with the former target allowlist must
// be rebuilt (Issue #10835).
// 191: runtime-nominal operands carry the reserved concrete type and dormant
// inner-constructor function rows needed for branch-gated publication, and
// reached-prefix metadata retains those constructor activations (#11679).
const CACHE_VERSION: u32 = 191;
const BASE_CACHE_MAGIC: [u8; 8] = *b"SJBCACH1";
const SECTION_LEN_BYTES: usize = std::mem::size_of::<u64>();

/// Serialize a value for a Base cache section/header payload (Issue #10118).
///
/// `postcard` is a plain serde codec (same `Serialize`/`Deserialize` derives
/// as the historical bincode-varint codec, no layout/derive changes needed)
/// that uses LEB128 varints for integers by default — measured ~3-5x faster
/// to decode than bincode's varint mode, with a smaller payload, since it
/// skips bincode's extra per-value framing overhead. The section framing
/// (`u64` length prefixes in `append_section_bytes`/`read_section`) is
/// written/read as raw little-endian bytes and is independent of this codec.
pub(crate) fn cache_serialize<T: Serialize>(value: &T) -> Result<Vec<u8>, String> {
    postcard::to_allocvec(value).map_err(|e| format!("postcard serialize failed: {e}"))
}

/// Deserialize a value from an EXACT byte slice (a whole section already
/// isolated by `read_section`'s length-prefix framing, or the full buffer for
/// a one-shot decode). Mirrors [`cache_serialize`].
pub(crate) fn cache_deserialize<T: DeserializeOwned>(bytes: &[u8]) -> Result<T, String> {
    postcard::from_bytes(bytes).map_err(|e| format!("postcard deserialize failed: {e}"))
}

/// Deserialize a value as a PREFIX of `bytes`, returning the value and the
/// unconsumed remainder — the postcard equivalent of the historical
/// `cache_codec().deserialize_from(&mut cursor)` + `allow_trailing_bytes()`
/// streaming read used for the version-gate header trick in
/// [`deserialize_base_cache`]: read just the leading header without requiring
/// (or re-parsing) the rest of the buffer.
pub(crate) fn cache_deserialize_prefix<T: DeserializeOwned>(
    bytes: &[u8],
) -> Result<(T, &[u8]), String> {
    postcard::take_from_bytes(bytes).map_err(|e| format!("postcard prefix deserialize failed: {e}"))
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
    /// Generic-function name → its method table (Issue #9197 S7). The key is the
    /// typed [`MethodTableKey`] (was a bare `String`, the last string-keyed
    /// dispatch structure at the cache boundary). `serialize_base_cache` writes
    /// this section pre-sorted by key so it is deterministic across processes —
    /// the raw `HashMap` serialize used by `append_section` would otherwise
    /// iterate in per-process hash-seed order, bypassing this `sorted_hashmap`
    /// attribute (which only fires on the whole-struct serialize path).
    #[serde(serialize_with = "super::sorted_serde::sorted_hashmap")]
    pub(crate) method_tables: HashMap<MethodTableKey, MethodTable>,
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
    /// Hash of the wire-format enum variant-name lists (Issue #8626).
    /// See [`enum_variant_fingerprint`].
    enum_variant_fingerprint: String,
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
pub fn compute_prelude_hash() -> String {
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
/// fingerprints the Rust sources of this crate plus the payload-dependency
/// crates (`subset_julia_vm_ir`/`_types`/`_bytecode`, whose serde-derived
/// types appear in serialized payloads — Issue #10332) into
/// `SJULIA_BASE_CACHE_BUILD_HASH` and hashes schema-sensitive source files into
/// `SJULIA_BASE_CACHE_SCHEMA_HASH`; combine them with the Base/prelude source
/// hash so persistent and embedded caches miss after compiler/runtime changes
/// even when Base source is unchanged (Issues #7515/#8444).
pub fn compute_base_cache_hash() -> String {
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
pub fn base_cache_schema_fingerprint() -> String {
    env!("SJULIA_BASE_CACHE_SCHEMA_HASH").to_string()
}

/// Fingerprint of the compiler/VM build: build-script hash of all Rust
/// sources in `subset_julia_vm/src` plus the payload-dependency crates
/// (`subset_julia_vm_ir`, `subset_julia_vm_types`, `subset_julia_vm_bytecode`
/// — see `CACHE_BUILD_FINGERPRINT_ROOTS` in `build.rs`, Issue #10332). Those
/// crates define serde-derived payload types (`Program`/`Expr`/`JuliaType`/
/// `TypeExpr`/`Span`/`CompiledProgram`) not fully covered by the schema
/// manifest or the enum-variant fingerprint, so they must invalidate here.
pub fn compiler_build_fingerprint() -> &'static str {
    env!("SJULIA_BASE_CACHE_BUILD_HASH")
}

/// Fingerprint of the wire-format enum variant lists (Issue #8626).
///
/// bincode is a positional format: an enum value is encoded as its variant
/// *declaration index* plus payload. Serialized caches (Base bytecode via
/// `Instr`/`BuiltinId`/`Intrinsic`, prelude Program and
/// `SpecializableFunction.ir` via `BuiltinOp`) therefore change meaning
/// whenever variants are inserted, removed, or reordered — historically a
/// silent corruption where old caches decoded into *different* instructions.
///
/// This hashes each enum's variant names in declaration order (via
/// `strum::VariantNames`, generated at compile time from the actual enum
/// declarations, so it cannot drift). The cache loaders embed it in the cache
/// header and reject any cache whose fingerprint differs, falling back to
/// recompilation from source instead of misdecoding.
///
/// Embedded caches (iOS/`SJULIA_BASE_CACHE`, `SJULIA_PRELUDE_PROGRAM_CACHE`)
/// are generated by a binary built from the same source tree in `build.sh`,
/// so their fingerprints match by construction; the check still runs and
/// falls back gracefully if a foreign cache file is ever embedded.
pub fn enum_variant_fingerprint() -> String {
    static FINGERPRINT: once_cell::sync::Lazy<String> = once_cell::sync::Lazy::new(|| {
        use strum::VariantNames;

        fn hash_enum(hasher: &mut Sha256, enum_name: &str, variants: &[&str]) {
            hasher.update(enum_name.as_bytes());
            hasher.update(b"\0");
            for variant in variants {
                hasher.update(variant.as_bytes());
                hasher.update(b"\x1f");
            }
            hasher.update(b"\0");
        }

        let mut hasher = Sha256::new();
        hash_enum(&mut hasher, "Instr", Instr::VARIANTS);
        hash_enum(
            &mut hasher,
            "BuiltinId",
            crate::builtins::BuiltinId::VARIANTS,
        );
        hash_enum(
            &mut hasher,
            "Intrinsic",
            crate::intrinsics::Intrinsic::VARIANTS,
        );
        hash_enum(
            &mut hasher,
            "BuiltinOp",
            crate::ir::core::BuiltinOp::VARIANTS,
        );
        format!("{:x}", hasher.finalize())
    });
    FINGERPRINT.clone()
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
        enum_variant_fingerprint: enum_variant_fingerprint(),
    };

    let mut bytes = cache_serialize(&header)
        .map_err(|e| format!("Base cache header serialization failed: {}", e))?;
    append_compiled_program_section(&mut bytes, compiled)?;
    // Issue #9197 S7: serialize the method-table map as entries sorted by the
    // typed `MethodTableKey`, so the section is byte-deterministic across
    // processes. `append_section` serializes the raw value; a `HashMap` would
    // iterate in per-process hash-seed order (the #9473-class determinism bug —
    // the struct's `sorted_hashmap` attribute only fires on a whole-struct
    // serialize, never on this section path). A sorted `Vec<(K, &V)>` writes the
    // same bincode wire a map would (length-prefixed `(k, v)` entries), so it
    // still decodes into the `HashMap<MethodTableKey, MethodTable>` field.
    let mut method_table_entries: Vec<(MethodTableKey, &MethodTable)> = method_tables
        .iter()
        .map(|(name, table)| (MethodTableKey::new(name.clone()), table))
        .collect();
    method_table_entries.sort_by(|a, b| a.0.cmp(&b.0));
    append_section(&mut bytes, "method_tables", &method_table_entries)?;
    // Issue #9197 S7 / #9473: closure_captures shares method_tables' latent
    // non-determinism — `append_section` serializes the raw `HashMap<String,
    // HashSet<String>>` in per-process hash-seed order, bypassing the
    // `sorted_hashmap_of_hashset` attribute (which only fires on a whole-struct
    // serialize). Emit outer keys and inner capture names sorted so the whole
    // Base cache is byte-deterministic across processes. A `Vec<(K, Vec<V>)>`
    // writes the same bincode wire the map/set would (length-prefixed entries),
    // so it still decodes into the `HashMap<String, HashSet<String>>` field.
    let mut closure_capture_entries: Vec<(&String, Vec<&String>)> = closure_captures
        .iter()
        .map(|(name, captures)| {
            let mut inner: Vec<&String> = captures.iter().collect();
            inner.sort();
            (name, inner)
        })
        .collect();
    closure_capture_entries.sort_by(|a, b| a.0.cmp(b.0));
    append_section(&mut bytes, "closure_captures", &closure_capture_entries)?;
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
/// Read first — via [`cache_deserialize_prefix`], which (like postcard's
/// underlying `take_from_bytes`) does not require consuming the whole buffer —
/// so an incompatible older snapshot is rejected at the version gate *before*
/// the full positional decode. The wire format is positional, so a later
/// layout change (e.g. `CachedReturn.method_edges`, #5967) would otherwise
/// misalign every following field and surface as a cryptic decode error or
/// silent garbage (Issue #5968). `version` is the cache envelope's first
/// field, so this reads the same leading bytes without decoding the rest of
/// the envelope or payload.
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
    let (header, _): (CacheVersionHeader, _) =
        super::profile::time_immediate("cache.deserialize_header", || {
            cache_deserialize_prefix(bytes)
        })
        .map_err(|e| format!("Cache header read failed: {}", e))?;
    if header.version != CACHE_VERSION {
        return Err(format!(
            "Cache version mismatch: expected {}, got {}",
            CACHE_VERSION, header.version
        ));
    }

    let (header, remainder): (CacheEnvelopeHeader, _) =
        super::profile::time_immediate("cache.deserialize_envelope", || {
            cache_deserialize_prefix(bytes)
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
    // Enum variant fingerprint gate (Issue #8626): reject caches whose
    // bytecode/IR enums were declared in a different order BEFORE any payload
    // decode — a mismatched cache would otherwise misdecode discriminants into
    // different instructions instead of failing.
    let current_enum_fingerprint = enum_variant_fingerprint();
    if header.enum_variant_fingerprint != current_enum_fingerprint {
        return Err(format!(
            "Base cache enum variant fingerprint mismatch: expected {}, got {}",
            current_enum_fingerprint, header.enum_variant_fingerprint
        ));
    }

    // Authoritative "full bincode decode" total (Issue #9201). Every persisted
    // sub-payload of `SerializedBaseCache` is decoded inside this single timer,
    // so the profile summary carries one number for the whole decode alongside
    // the per-section breakdown recorded by the helpers below. That total, over
    // the compile wall printed by `print_summary`, is the decode-share the
    // Performance Decision Protocol thresholds on. All three load paths
    // (embedded / persistent on-disk / `SJULIA_BASE_CACHE`) reach here through
    // `deserialize_base_cache`, so the phase is measured identically on each.
    let mut offset = bytes.len() - remainder.len();
    let (compiled, method_tables, closure_captures, promotion_rules, inference_results) =
        super::profile::time_immediate(
            "cache.base_cache_decode_total",
            || -> Result<_, String> {
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
                Ok((
                    compiled,
                    method_tables,
                    closure_captures,
                    promotion_rules,
                    inference_results,
                ))
            },
        )?;

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
        super::profile::time_immediate("cache.compute_base_cache_hash", compute_base_cache_hash);
    if cache.source_hash != current_hash {
        return Err(
            "Source hash mismatch: cache was built with different Base source or compiler"
                .to_string(),
        );
    }

    record_cache_profile(&cache, bytes.len());

    Ok(cache)
}

/// Width-portable stand-in for the `usize::MAX` "no fallback method" sentinel
/// carried by `Instr::CallDynamic`'s `fallback_func_index` (Issue #9235).
///
/// The Base cache is generated by a 64-bit host (`usize::MAX == 2^64-1`) but the
/// WASM Playground consumes it on `wasm32`, where `usize` is 32-bit and cannot
/// decode `2^64-1` — bincode rejects the whole `compiled.code` section, so the
/// embedded cache is silently discarded and Base is recompiled from source on
/// every first run (~8 s cold start). iOS/native are 64-bit so never hit it.
///
/// Real function indices are always far below `u32::MAX` (the VM already relies
/// on this for `CALL_SITE_WAY_SENTINEL`), so encoding the sentinel as
/// `u32::MAX as usize` is unambiguous and fits every target's `usize`. We remap
/// `usize::MAX <-> this` only at the cache serialize/deserialize boundary; the
/// VM keeps using `usize::MAX` everywhere, so no dispatch logic changes.
pub(crate) const PORTABLE_CALL_DYNAMIC_NO_FALLBACK: usize = u32::MAX as usize;

/// Rewrite `compiled.code` for serialization so the width-dependent
/// `usize::MAX` sentinel becomes the width-portable one (Issue #9235). Only
/// `Instr::CallDynamic`'s fallback index carries `usize::MAX` in finalized Base
/// bytecode (verified by `--dump-bytecode --all`); `PushI128(u64::MAX)` and
/// friends are genuine i128/data literals and are left untouched.
fn code_with_portable_sentinels(code: &[Instr]) -> Vec<Instr> {
    code.iter()
        .map(|instr| match instr {
            Instr::CallDynamic(operands) if operands.fallback_func_index == usize::MAX => {
                let mut portable = operands.clone();
                portable.fallback_func_index = PORTABLE_CALL_DYNAMIC_NO_FALLBACK;
                Instr::CallDynamic(portable)
            }
            other => other.clone(),
        })
        .collect()
}

/// Inverse of [`code_with_portable_sentinels`]: restore the runtime `usize::MAX`
/// sentinel after decoding, so the VM sees its canonical value on every target.
fn restore_native_sentinels(code: &mut [Instr]) {
    for instr in code.iter_mut() {
        if let Instr::CallDynamic(operands) = instr {
            if operands.fallback_func_index == PORTABLE_CALL_DYNAMIC_NO_FALLBACK {
                operands.fallback_func_index = usize::MAX;
            }
        }
    }
}

fn append_compiled_program_section(
    bytes: &mut Vec<u8>,
    compiled: &CompiledProgram,
) -> Result<(), String> {
    let mut payload = cache_serialize(&CompiledProgramHeader {
        entry: compiled.entry,
        base_function_count: compiled.base_function_count,
        global_slot_count: compiled.global_slot_count,
    })
    .map_err(|e| format!("Base cache compiled header serialization failed: {e}"))?;

    // Encode a width-portable copy of the bytecode so `wasm32` (32-bit usize)
    // can decode the `CallDynamic` no-fallback sentinel (Issue #9235).
    let portable_code = code_with_portable_sentinels(&compiled.code);
    append_section(&mut payload, "compiled.code", &portable_code)?;
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
    append_section(&mut payload, "compiled.enum_defs", &compiled.enum_defs)?;
    append_section(
        &mut payload,
        "compiled.show_methods",
        &compiled.show_methods,
    )?;
    append_section(
        &mut payload,
        "compiled.print_methods",
        &compiled.print_methods,
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
        "compiled.inference_global_types_snapshot",
        &compiled.inference_global_types_snapshot,
    )?;
    append_section(
        &mut payload,
        "compiled.specialization_disable_flags",
        &compiled.specialization_disable_flags,
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
    // Immediate (not buffered) so the timing survives on the cold CLI path,
    // where the decode runs in `warm_base_cache` before `compile_with_cache`'s
    // `reset()` would wipe buffered events (Issue #9201 / #6348).
    super::profile::time_immediate(time_label, || deserialize_compiled_program_body(section))
}

fn deserialize_compiled_program_body(bytes: &[u8]) -> Result<CompiledProgram, String> {
    let (header, remainder): (CompiledProgramHeader, _) =
        super::profile::time_immediate("cache.deserialize.compiled.header", || {
            cache_deserialize_prefix(bytes)
        })
        .map_err(|e| format!("Base cache compiled header decode failed: {e}"))?;
    let mut offset = bytes.len() - remainder.len();

    let mut code: Vec<Instr> = deserialize_section(
        bytes,
        &mut offset,
        "compiled.code",
        "cache.deserialize.compiled.code",
        "cache.section.compiled.code_bytes",
    )?;
    // Restore the native `usize::MAX` no-fallback sentinel (Issue #9235).
    restore_native_sentinels(&mut code);
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
    let enum_defs = deserialize_section(
        bytes,
        &mut offset,
        "compiled.enum_defs",
        "cache.deserialize.compiled.enum_defs",
        "cache.section.compiled.enum_defs_bytes",
    )?;
    let show_methods = deserialize_section(
        bytes,
        &mut offset,
        "compiled.show_methods",
        "cache.deserialize.compiled.show_methods",
        "cache.section.compiled.show_methods_bytes",
    )?;
    let print_methods = deserialize_section(
        bytes,
        &mut offset,
        "compiled.print_methods",
        "cache.deserialize.compiled.print_methods",
        "cache.section.compiled.print_methods_bytes",
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
    let inference_global_types_snapshot = deserialize_section(
        bytes,
        &mut offset,
        "compiled.inference_global_types_snapshot",
        "cache.deserialize.compiled.inference_global_types_snapshot",
        "cache.section.compiled.inference_global_types_snapshot_bytes",
    )?;
    let specialization_disable_flags = deserialize_section(
        bytes,
        &mut offset,
        "compiled.specialization_disable_flags",
        "cache.deserialize.compiled.specialization_disable_flags",
        "cache.section.compiled.specialization_disable_flags_bytes",
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
        source_map: Vec::new(),
        functions,
        struct_defs,
        abstract_types,
        primitive_types,
        enum_defs,
        show_methods,
        print_methods,
        entry: header.entry,
        specializable_functions,
        runtime_specialization_map,
        inference_global_types_snapshot,
        specialization_disable_flags,
        compile_context: None,
        base_function_count: header.base_function_count,
        // Neither `macro_bindings` nor `module_registry` is a section in this
        // format (Issue #10988 Phase 2a: never was, for `macro_bindings` —
        // see the `CACHE_VERSION` 134 changelog comment above). Both are
        // rebuilt fresh by the compile pipeline (`collect_module_metadata`/
        // `finalize`) for whatever program reuses this Base-cache prefix.
        macro_bindings: std::collections::HashMap::new(),
        module_registry: Default::default(),
        global_slot_names,
        global_slot_types,
        global_slot_count: header.global_slot_count,
        // Runtime-only field (never serialized): a deserialized Base cache is a
        // prefix; the real per-eval snapshot is set fresh by `finalize` for the
        // user main block (Issue #9182).
        main_scope_names: Default::default(),
    })
}

fn append_section<T: Serialize>(
    bytes: &mut Vec<u8>,
    section_name: &'static str,
    value: &T,
) -> Result<(), String> {
    let payload = cache_serialize(value)
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
    // Immediate print: the decode runs pre-`reset()` on the cold CLI path
    // (Issue #9201), so buffered timings would be discarded.
    super::profile::time_immediate(time_label, || cache_deserialize(section))
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
    super::profile::note_immediate(bytes_label, || format!("{} bytes", section.len()));
    Ok(section)
}

fn record_cache_profile(cache: &SerializedBaseCache, total_bytes: usize) {
    // Immediate: `deserialize_base_cache` runs before `compile_with_cache`'s
    // `reset()` on the cold CLI path (Issue #9201), so buffered notes vanish.
    super::profile::note_immediate("cache.base_cache_total_bytes", || total_bytes.to_string());
    super::profile::note_immediate("cache.base_cache_counts", || {
        let method_count: usize = cache
            .method_tables
            .values()
            .map(|table| table.methods.len())
            .sum();
        let closure_binding_count: usize = cache.closure_captures.values().map(HashSet::len).sum();
        format!(
            "code={} functions={} structs={} abstracts={} primitive_types={} enums={} show_methods={} print_methods={} persisted_specializable={} runtime_specialization_map={} method_tables={} methods={} closure_scopes={} closure_bindings={} promotion_rules={} inference_results={}",
            cache.compiled.code.len(),
            cache.compiled.functions.len(),
            cache.compiled.struct_defs.len(),
            cache.compiled.abstract_types.len(),
            cache.compiled.primitive_types.len(),
            cache.compiled.enum_defs.len(),
            cache.compiled.show_methods.len(),
            cache.compiled.print_methods.len(),
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
    // Warm Base directly. The general compile_with_cache path also initializes
    // the preload-package cache, which can make --precompile-base wait on (or
    // recursively contend for) a preload lock before the explicit package-cache
    // generation step starts (Issue #10196).
    super::cache::warm_base_cache();

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
fn empty_compiled_program() -> CompiledProgram {
    CompiledProgram {
        code: Vec::new(),
        source_map: Vec::new(),
        functions: Vec::new(),
        struct_defs: Vec::new(),
        abstract_types: Vec::new(),
        primitive_types: Vec::new(),
        enum_defs: Vec::new(),
        show_methods: Vec::new(),
        print_methods: Vec::new(),
        entry: 0,
        specializable_functions: Vec::new(),
        runtime_specialization_map: Vec::new(),
        inference_global_types_snapshot: Vec::new(),
        specialization_disable_flags: Default::default(),
        compile_context: None,
        base_function_count: 0,
        macro_bindings: std::collections::HashMap::new(),
        module_registry: Default::default(),
        global_slot_names: Vec::new(),
        global_slot_types: Vec::new(),
        global_slot_count: 0,
        main_scope_names: Default::default(),
    }
}

/// Build cache bytes whose envelope carries a deliberately wrong
/// `enum_variant_fingerprint` (Issue #8626) — used by `cache.rs` tests to
/// assert the persistent-cache load path falls back to source compilation
/// instead of panicking or misdecoding.
#[cfg(test)]
#[allow(clippy::unwrap_used, clippy::expect_used)]
pub(crate) fn cache_bytes_with_tampered_enum_fingerprint() -> Vec<u8> {
    let program = empty_compiled_program();
    let bytes = serialize_base_cache(&program, &HashMap::new(), &HashMap::new(), &[])
        .expect("serialization of empty program should succeed");

    let (mut header, remainder): (CacheEnvelopeHeader, _) =
        cache_deserialize_prefix(&bytes).expect("cache header should decode");
    header.enum_variant_fingerprint =
        "0000000000000000000000000000000000000000000000000000000000000000".to_string();

    let mut tampered = cache_serialize(&header).expect("rewritten header should encode");
    tampered.extend_from_slice(remainder);
    tampered
}

/// Build cache bytes whose envelope carries a deliberately wrong
/// `compiler_build_fingerprint` (Issue #8718) so the persistent-cache load path
/// proves it regenerates when a different sjulia binary produced the cache.
#[cfg(test)]
#[allow(clippy::unwrap_used, clippy::expect_used)]
pub(crate) fn cache_bytes_with_tampered_compiler_build_fingerprint() -> Vec<u8> {
    let program = empty_compiled_program();
    let bytes = serialize_base_cache(&program, &HashMap::new(), &HashMap::new(), &[])
        .expect("serialization of empty program should succeed");

    let (mut header, remainder): (CacheEnvelopeHeader, _) =
        cache_deserialize_prefix(&bytes).expect("cache header should decode");
    header.compiler_build_fingerprint =
        "0000000000000000000000000000000000000000000000000000000000000000".to_string();

    let mut tampered = cache_serialize(&header).expect("rewritten header should encode");
    tampered.extend_from_slice(remainder);
    tampered
}

#[cfg(test)]
#[allow(clippy::unwrap_used, clippy::expect_used)]
mod tests {
    use super::*;

    // ── postcard wire-format migration (Issue #10118) ──────────────────────────

    /// Pins the exact bytes `cache_serialize` produces for a small, fully
    /// deterministic struct. Base cache sections are `postcard`-encoded
    /// (Issue #10118, replacing bincode's varint mode); this catches an
    /// unreviewed change to `postcard`'s own wire format (a dependency bump
    /// that altered its LEB128 varint/struct-field encoding) that would
    /// otherwise silently produce caches only the exact same postcard version
    /// can decode. A deliberate, reviewed postcard version bump that changes
    /// this must also bump `CACHE_VERSION` and update the pinned bytes here.
    #[test]
    fn postcard_wire_format_is_pinned_10118() {
        // u32 93 as a postcard/LEB128 varint: fits in 7 bits (93 < 128), so a
        // single byte with the continuation bit (0x80) clear.
        let version_bytes = cache_serialize(&CacheVersionHeader { version: 93 })
            .expect("CacheVersionHeader should serialize");
        assert_eq!(
            version_bytes,
            vec![93u8],
            "CacheVersionHeader(93) wire bytes must stay pinned to a single LEB128 byte"
        );

        // A value requiring a second varint byte (300 = 0b1_0010_1100):
        // low 7 bits (0b010_1100 = 0x2C) with the continuation bit set
        // (0xAC), then the remaining bits (0b10 = 0x02).
        let big_version_bytes = cache_serialize(&CacheVersionHeader { version: 300 })
            .expect("CacheVersionHeader should serialize");
        assert_eq!(
            big_version_bytes,
            vec![0xAC, 0x02],
            "CacheVersionHeader(300) wire bytes must stay pinned to a 2-byte LEB128 varint"
        );

        // Round-trip via the streaming prefix reader used by the version gate.
        let (decoded, remainder): (CacheVersionHeader, _) =
            cache_deserialize_prefix(&version_bytes).expect("should decode back");
        assert_eq!(decoded.version, 93);
        assert!(remainder.is_empty());
    }

    // ── width-portable CallDynamic sentinel (Issue #9235) ──────────────────────

    /// The serialized Base cache must never contain a `CallDynamic` fallback of
    /// `usize::MAX`: on `wasm32` (32-bit `usize`) that value (`2^64-1` on the
    /// 64-bit generator) cannot be decoded and bincode rejects the whole
    /// `compiled.code` section, silently discarding the embedded cache and
    /// forcing Base recompilation on every first run. The portable encoding
    /// must remap it to a value that fits `u32`, and the inverse must restore
    /// the runtime sentinel exactly.
    #[test]
    fn call_dynamic_no_fallback_sentinel_is_width_portable_9235() {
        let code = vec![
            Instr::call_dynamic("f", usize::MAX, 2, Vec::new()),
            Instr::call_dynamic("g", 7, 1, Vec::new()),
            // A genuine i128 data literal that happens to equal u64::MAX must be
            // left untouched (it is not a usize sentinel).
            Instr::PushI128(Box::new(u64::MAX as i128)),
        ];

        let portable = code_with_portable_sentinels(&code);
        for instr in &portable {
            if let Instr::CallDynamic(operands) = instr {
                assert_ne!(
                    operands.fallback_func_index,
                    usize::MAX,
                    "serialized CallDynamic fallback must not be usize::MAX (breaks wasm32 decode)"
                );
                assert!(
                    operands.fallback_func_index <= u32::MAX as usize,
                    "serialized CallDynamic fallback must fit u32 for wasm32"
                );
            }
        }
        // The no-fallback entry is remapped; the real index is preserved.
        assert!(matches!(&portable[0], Instr::CallDynamic(operands)
            if operands.fallback_func_index == PORTABLE_CALL_DYNAMIC_NO_FALLBACK
                && operands.arg_count == 2));
        assert!(matches!(&portable[1], Instr::CallDynamic(operands)
            if operands.fallback_func_index == 7 && operands.arg_count == 1));
        // Data literals are not sentinels and are never rewritten.
        assert!(matches!(&portable[2], Instr::PushI128(v) if **v == u64::MAX as i128));

        let mut restored = portable;
        restore_native_sentinels(&mut restored);
        assert!(matches!(&restored[0], Instr::CallDynamic(operands)
            if operands.fallback_func_index == usize::MAX && operands.arg_count == 2));
        assert!(matches!(&restored[1], Instr::CallDynamic(operands)
            if operands.fallback_func_index == 7 && operands.arg_count == 1));
    }

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

    #[test]
    fn test_enum_variant_fingerprint_is_64_hex_and_deterministic_8626() {
        let fingerprint1 = enum_variant_fingerprint();
        let fingerprint2 = enum_variant_fingerprint();
        assert_eq!(
            fingerprint1, fingerprint2,
            "enum variant fingerprint must be deterministic"
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
            base_cache_schema_fingerprint(),
            "enum variant fingerprint must be independent of the schema file hash"
        );
    }

    /// Issue #8626: a cache built with a different `Instr`/`BuiltinId`/
    /// `Intrinsic`/`BuiltinOp` variant order must be rejected cleanly at the
    /// header gate — before any bytecode payload decode — so the caller
    /// regenerates it from source instead of executing misdecoded bytecode.
    #[test]
    fn test_stale_enum_variant_fingerprint_is_rejected_8626() {
        let tampered = cache_bytes_with_tampered_enum_fingerprint();
        let err_msg = deserialize_base_cache(&tampered)
            .expect_err("mismatched enum variant fingerprint must reject the cache");
        assert!(
            err_msg.contains("enum variant fingerprint mismatch"),
            "expected enum-variant-fingerprint rejection, got: {}",
            err_msg
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
    fn test_serialize_deserialize_roundtrip_compiled_sections() {
        use std::collections::HashMap;

        let mut program = empty_compiled_program();
        program.enum_defs = vec![crate::bytecode::EnumDefInfo {
            name: "CacheEnum9784".to_string(),
            base_type: "UInt8".to_string(),
            members: vec![
                ("cache_enum_zero9784".to_string(), 0),
                ("cache_enum_one9784".to_string(), 1),
            ],
        }];
        program.inference_global_types_snapshot = vec![
            (
                "CacheConst10333".to_string(),
                crate::bytecode::ValueType::I64,
            ),
            (
                "CacheMutable10333".to_string(),
                crate::bytecode::ValueType::Any,
            ),
        ];
        program.specialization_disable_flags = SpecializationDisableFlags {
            array_getindex: true,
            array_setindex: false,
            field_access: true,
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
        assert_eq!(
            cache.compiled.inference_global_types_snapshot,
            vec![
                (
                    "CacheConst10333".to_string(),
                    crate::bytecode::ValueType::I64
                ),
                (
                    "CacheMutable10333".to_string(),
                    crate::bytecode::ValueType::Any
                ),
            ],
            "Base-cache compiled sections must round-trip inference globals (Issue #10333)"
        );
        assert_eq!(
            cache.compiled.specialization_disable_flags,
            SpecializationDisableFlags {
                array_getindex: true,
                array_setindex: false,
                field_access: true,
            },
            "Base-cache compiled sections must round-trip specialization policy (Issue #10334)"
        );
        assert_eq!(
            cache.compiled.enum_defs,
            vec![crate::bytecode::EnumDefInfo {
                name: "CacheEnum9784".to_string(),
                base_type: "UInt8".to_string(),
                members: vec![
                    ("cache_enum_zero9784".to_string(), 0),
                    ("cache_enum_one9784".to_string(), 1),
                ],
            }],
            "Base-cache compiled sections must round-trip enum metadata (Issue #9784)"
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

    include!("../../tests/internal/cache_helper_provenance_9784_test.rs");

    /// Issue #9197 S7 / #9473: the Base-cache method-table section serializes
    /// deterministically (byte-identical regardless of `HashMap` insertion /
    /// hash-seed iteration order) and round-trips through the typed
    /// `MethodTableKey`, preserving every generic-function name. Two
    /// independently-built maps with opposite insertion orders — which get
    /// distinct per-`HashMap` hash seeds, hence distinct raw iteration orders —
    /// must produce identical bytes because `serialize_base_cache` emits the
    /// section pre-sorted by key. This is the cross-process determinism property
    /// the raw (pre-S7) `append_section` `HashMap` serialize did NOT have.
    #[test]
    fn method_tables_serialize_deterministically_with_typed_key_issue_9197_s7() {
        use crate::compile::MethodTable;
        use std::collections::HashMap;

        // Representative generic-function-name keys: operator, bare, module-
        // qualified, constructor bare type name, nested-qualified.
        let names = [
            "+",
            "log2",
            "Base.log2",
            "Module.f",
            "Foo",
            "parent#nested",
            "sin",
            "map",
        ];

        let mut forward: HashMap<String, MethodTable> = HashMap::new();
        for n in names.iter() {
            forward.insert((*n).to_string(), MethodTable::new((*n).to_string()));
        }
        let mut reverse: HashMap<String, MethodTable> = HashMap::new();
        for n in names.iter().rev() {
            reverse.insert((*n).to_string(), MethodTable::new((*n).to_string()));
        }

        let empty_captures = HashMap::new();
        let bytes_fwd =
            serialize_base_cache(&empty_compiled_program(), &forward, &empty_captures, &[])
                .expect("serialize forward-inserted map");
        let bytes_rev =
            serialize_base_cache(&empty_compiled_program(), &reverse, &empty_captures, &[])
                .expect("serialize reverse-inserted map");
        assert_eq!(
            bytes_fwd, bytes_rev,
            "Base cache bytes must be identical regardless of method_tables \
             insertion / hash-seed iteration order (Issue #9197 S7 / #9473)"
        );

        // Round-trip: the typed-key section decodes into the MethodTableKey-keyed
        // field with every generic-function name preserved losslessly.
        let cache = deserialize_base_cache(&bytes_fwd).expect("typed-key round-trip");
        assert_eq!(cache.method_tables.len(), names.len());
        for n in names.iter() {
            assert!(
                cache.method_tables.contains_key(&MethodTableKey::new(*n)),
                "method table for `{n}` must survive the typed-key round-trip"
            );
        }
        let mut got: Vec<String> = cache
            .method_tables
            .keys()
            .map(|k| k.as_str().to_string())
            .collect();
        got.sort();
        let mut want: Vec<String> = names.iter().map(|s| (*s).to_string()).collect();
        want.sort();
        assert_eq!(got, want, "typed keys must expose their canonical names");
    }

    /// Issue #10051 slice B: `closure_captures` (`HashMap<String, HashSet<String>>`)
    /// shares `method_tables`' latent non-determinism — `append_section` in
    /// `serialize_base_cache` serializes a pre-sorted `Vec<(&String, Vec<&String>)>`
    /// derived from the map (see the comment above the call site), never the raw
    /// `HashMap`/`HashSet` — but until this test, that property was only checked
    /// indirectly by the slow cross-process
    /// `precompile_base_is_deterministic_across_processes` integration test. This
    /// in-process test pins it directly and cheaply: two maps holding the same
    /// scope→macro-name entries, built with opposite outer insertion order AND
    /// opposite inner (per-scope) insertion order, must serialize to identical
    /// bytes, and must round-trip every scope/name pair losslessly.
    #[test]
    fn closure_captures_serialize_deterministically_regardless_of_insertion_order_issue_10051() {
        use std::collections::{HashMap, HashSet};

        // Representative scope names (mirrors closures/nested-function naming,
        // "parent#nested") each owning a handful of captured-variable names.
        let scopes: [(&str, &[&str]); 4] = [
            ("outer#inner", &["x", "acc", "y"]),
            ("Module.helper", &["state"]),
            ("f#g#h", &["a", "b", "c", "d"]),
            ("main", &["total"]),
        ];

        let mut forward: HashMap<String, HashSet<String>> = HashMap::new();
        for (scope, names) in scopes.iter() {
            forward.insert(
                (*scope).to_string(),
                names.iter().map(|n| n.to_string()).collect(),
            );
        }
        // Reverse both outer scope order and inner capture-name order, so a
        // non-deterministic serializer would very likely disagree on both axes.
        let mut reverse: HashMap<String, HashSet<String>> = HashMap::new();
        for (scope, names) in scopes.iter().rev() {
            let inner: HashSet<String> = names.iter().rev().map(|n| n.to_string()).collect();
            reverse.insert((*scope).to_string(), inner);
        }

        let empty_tables = HashMap::new();
        let bytes_fwd =
            serialize_base_cache(&empty_compiled_program(), &empty_tables, &forward, &[])
                .expect("serialize forward-inserted closure_captures");
        let bytes_rev =
            serialize_base_cache(&empty_compiled_program(), &empty_tables, &reverse, &[])
                .expect("serialize reverse-inserted closure_captures");
        assert_eq!(
            bytes_fwd, bytes_rev,
            "Base cache bytes must be identical regardless of closure_captures outer/inner \
             insertion order (Issue #10051 slice B / #9473-class determinism)"
        );

        let cache = deserialize_base_cache(&bytes_fwd).expect("closure_captures round-trip");
        assert_eq!(cache.closure_captures.len(), scopes.len());
        for (scope, names) in scopes.iter() {
            let got = cache
                .closure_captures
                .get(*scope)
                .unwrap_or_else(|| panic!("scope `{scope}` must survive the round-trip"));
            let want: HashSet<String> = names.iter().map(|n| n.to_string()).collect();
            assert_eq!(
                got, &want,
                "captured names for `{scope}` must round-trip exactly"
            );
        }
    }

    /// Issue #8444: a cache produced with the current `CACHE_VERSION` but an old
    /// bytecode/method-table schema must fail cleanly before its payload can be
    /// reused as if it were compatible.
    #[test]
    fn test_stale_cache_schema_fingerprint_is_rejected_8444() {
        use std::collections::HashMap;

        let program = empty_compiled_program();

        let bytes = serialize_base_cache(&program, &HashMap::new(), &HashMap::new(), &[])
            .expect("serialization of empty program should succeed");
        let (mut header, remainder): (CacheEnvelopeHeader, _) =
            cache_deserialize_prefix(&bytes).expect("cache header should decode");
        header.schema_fingerprint =
            "0000000000000000000000000000000000000000000000000000000000000000".to_string();

        let mut stale_bytes = cache_serialize(&header).expect("rewritten header should encode");
        stale_bytes.extend_from_slice(remainder);

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
        use std::collections::HashMap;

        // Build a cache with a wrong version number
        let wrong_version_cache = SerializedBaseCache {
            version: CACHE_VERSION + 1,
            source_hash: compute_prelude_hash(),
            compiled: empty_compiled_program(),
            method_tables: HashMap::new(),
            closure_captures: HashMap::new(),
            promotion_rules: Vec::new(),
            inference_results: Vec::new(),
        };

        let bytes = cache_serialize(&wrong_version_cache)
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
        // A leading u32 == CACHE_VERSION - 1 (version is encoded first), then
        // bytes that do NOT form a valid current SerializedBaseCache.
        let mut bytes = cache_serialize(&CacheVersionHeader {
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
        let bytes = cache_serialize(&CacheVersionHeader {
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
