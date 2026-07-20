# Cache Representation RFC: IR, Bytecode, and Incremental Layers

Status: Accepted (Issue #10051)

## Decision

SubsetJuliaVM keeps a hybrid cache architecture. It does not replace every
cache with one cache-as-IR format:

- the prelude cache stores lowered `Program` IR;
- the Base cache stores validated compiled bytecode and the specializable Core
  IR needed to rebuild runtime specialization context;
- preload-package bytecode is an opt-in, exact-layout acceleration layer;
- inference entries carry explicit `WorldRange` validity;
- bundled `.sjvmbc` artifacts are immutable compiler-build products with the
  same schema/compiler/enum fingerprints as their producer.

This is closer to upstream Julia's split between lowered/source IR,
`CodeInstance` validity, and native/code caches than a single undifferentiated
serialized blob. Each lane owns a different reuse boundary and must fail closed
when that boundary cannot be proven.

## Why not a wholesale cache-as-IR migration

The Base cache already avoids reparsing/lowering and its bytecode is immediately
executable. Replacing it with only higher-level IR would move inference/codegen
back onto every cold start and would not eliminate deserialization. The cache
also already embeds Core IR for functions that may be runtime-specialized, so
the useful IR/bytecode split exists today.

Lazy per-function Base decode remains an independent performance opportunity
(Issue #9395). It can change section indexing/decoding without changing the
semantic representation decision here.

## Preload package isolation

Preload bytecode contains frozen function and struct indices. A separate ID
namespace plus complete relocation is possible, but is not the accepted default
until every referenced index space has a relocation proof. Two historical
silent-corruption bugs (#9254 and #9646) demonstrate that partial relocation is
worse than a cache miss.

The accepted safety boundary is therefore:

1. preload generation and embedding are off by default;
2. operators must opt into an exact package list and order;
3. the complete non-Base closure layout must match;
4. any top-level user struct or other unproved index shift disables splicing;
5. a miss compiles normally and must be byte-for-byte behaviorally equivalent.

Issue #9477 tracks the optional performance work to make more program shapes
relocatable. It may broaden the hit rate only after adding a generalized layout
proof; it is not required for correctness or for #10051 closure.

## Cache validity and world changes

Validity is lane-specific:

- inference cache entries use `WorldRange` and precise/conservative backedge
  invalidation when methods change;
- Base and preload artifacts are immutable snapshots, validated by source,
  schema, compiler-build, enum, and closure-layout fingerprints;
- source-visible top-level redefinitions are resolved against the live method
  table; cached candidate metadata is refreshed or bypassed rather than treated
  as a valid older world;
- restore lanes rebuild/replay compiler context and are differential-tested
  against a fresh compile.

Stamping the whole immutable Base artifact with the mutable REPL world would
conflate these layers and invalidate unaffected Base code on every definition.
World validity belongs on inference/specialization entries; artifact identity
belongs on serialized cache envelopes.

## Determinism and schema evolution

All hash-backed collections entering a serialized section are canonicalized or
skipped. Independent-process tests compare complete Base-cache bytes. Schema
inputs are enumerated in `base_cache_schema_files.txt`; build-time fingerprints,
the checked snapshot audit, and the in-suite negative guard all derive from
those sources. A wire-shape change requires `CACHE_VERSION` plus a regenerated
snapshot, while runtime fingerprints reject stale artifacts even if a developer
forgets the review snapshot.

## Compile/VM boundary

Compiler code emits through the bytecode facade and does not import VM runtime
implementation. VM code consumes bytecode/runtime facades and does not import
compiler internals. Tests cross the boundary through crate-root test helpers.
`scripts/audit_compile_vm_coupling.sh` ratchets all three direct-coupling counts
to zero.

## Acceptance gates

The architecture is guarded by:

- `audit_base_cache_schema_fingerprint.sh` and the in-suite schema guard;
- `precompile_base_is_deterministic_across_processes`;
- preload top-level-struct/layout fail-closed tests (#9646/#9254);
- source-visible redefinition regressions (#9665);
- cache-restore parity tests (#10265);
- `audit_compile_vm_coupling.sh` and its negative self-tests.

Any future cache lane must declare its identity, validity, relocation, restore,
and miss behavior before it may persist executable state.
