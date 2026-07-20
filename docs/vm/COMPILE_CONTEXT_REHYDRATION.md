# Compile-Context Rehydration

**Status:** accepted design (Issue #10438, 2026-07-14). Production migration is
tracked by Issue #10462; this document is the authority for its boundaries and
ordering.

## Problem

Fresh compilation constructs semantic state outside the bytecode stream while
walking and compiling a `Program`. Cache consumers currently recover that state
through several lane-specific mechanisms: deserialize a field, derive it again
from the lowered program, replay thread-local registries, or leave a documented
gap. Adding a fresh-path mutation without an equivalent restore decision can
therefore make cache mode change dispatch, inference, reflection, GC behavior,
or REPL persistence.

The required invariant for source program `P` and cache lane `C` is:

```text
semantic_snapshot(fresh_compile(P))
==
semantic_snapshot(rehydrate(C, deserialize(C, serialize(C, fresh_compile(P)))))
```

Only explicitly non-semantic data such as allocation addresses and lane-local
dense IDs may be normalized. An unexplained difference is a failing cache lane,
not an allowed fallback.

This design complements [CACHE_ARCHITECTURE.md](./CACHE_ARCHITECTURE.md), which
defines wire formats, invalidation, and the existing cache-restore parity guard,
and [SEMANTIC_ID_MIGRATION.md](./SEMANTIC_ID_MIGRATION.md), which defines stable
owner-scoped IDs and relocation rules.

## Current baseline and known gaps

Phase 0 of #10462 has already landed a deterministic, test-only
`CompileContextSnapshot` in `compile/context_snapshot.rs`. It covers runtime
context presence, structs and parametric definitions, aliases, inference
globals, primitive types, module paths, main-scope names, method signatures,
the promotion registry, and specialization policy. The restore-lane scoreboard
in `compile/cache.rs` permits differences only when they name a tracking Issue.

As of 2026-07-18, the manual serialize/restore and `.sjvmbc` lanes intentionally
show these tracked differences:

| Snapshot field | Tracking Issue | Current cause |
|---|---:|---|
| promotion registry | #10339 | thread-local registry is not replayed by `.sjvmbc`/manual restore |
| `main_scope_names` | #10339 | `CompiledProgram` decode defaults the skipped field to empty |

`inference_global_types` is now a sorted persisted snapshot in
`CompiledProgram`; #10333 removed that scoreboard allowance and requires exact
fresh/manual/`.sjvmbc` equality. The finalized method-table decisions for array
`getindex`, array `setindex!`, and field access are likewise persisted in
`CompiledProgram::specialization_disable_flags`; #10334 removed the
specialization-policy allowance and requires exact equality instead. The
sectioned Base-cache format carries both snapshots as named sections. The
seeded-cache context-absence gap remains #10335, while promotion-registry and
main-scope hydration remain #10339. The scoreboard must not add a new allowance
without an Issue number, and removing a gap must replace its allowance with
equality.

## State inventory and ownership

Every semantic field belongs to exactly one of three restore classes. The class
is a property of the actual wire boundary, not of the Rust type or source file
where the field happens to be declared.

| Class | When to use | Restore rule | Examples |
|---|---|---|---|
| Persisted snapshot | The source information is not available after decode or recomputation could change meaning | Serialize a versioned, deterministically ordered section; validate it before use | inference globals, method/source-world state, promotion rules, specialization policy and reasons |
| Structural projection | The complete source of truth already survives the same boundary | Re-run one shared projector over the round-tripped structure in the same deterministic order | module registry derived from `Program.modules`, structurally derivable aliases/struct declarations |
| Runtime-only state | State is process-local or must not cross the boundary | Reinitialize explicitly and prove it cannot affect supported semantics, or replay a persisted semantic description without serializing runtime objects | caches, shared plans, GC-neutral accelerators |

It is forbidden to use `Default::default()`, an empty map, or fresh discovery as
an implicit fourth class. A default is valid only when an assertion proves it is
the semantic value for all supported programs, or an Issue-tracked scoreboard
entry records the temporary gap.

### Fresh-path mutation inventory

The migration must route these mutation families through typed context APIs:

| Family | Fresh source | Semantic consumers | Target restore class |
|---|---|---|---|
| modules and owner identity | module collection / interners | name resolution, dispatch, macros | structural projection or persisted relocation table, per wire shape |
| structs, parents, fields, constructors | struct-table construction | allocation, dispatch, reflection, GC-sensitive constructor behavior | typed events plus versioned struct snapshot |
| aliases and TypeVar/UnionAll graph | lowering/type collection | dispatch, reflection, parametric construction | typed events plus structured identity snapshot |
| functions, methods, signatures, source world | compiler/method-table registration | direct/dynamic calls, invalidation, reflection | persisted method snapshot with stable IDs |
| inference globals | inference engine after main optimization | reflection and runtime specialization | persisted inference snapshot |
| promotion rules | Base/user rule discovery | numeric dispatch and `promote` | persisted rules replayed into thread-local registry |
| specialization policy | whole-program override detection | native fast-path eligibility | persisted policy plus reason/provenance |
| main-scope names | compiled main scope | REPL/session persistence | persisted scope snapshot |
| GC ownership metadata | constructors and runtime values | WeakRef/finalizer semantics | semantic description only; never serialize live heap ownership accidentally |

## Typed mutation log

Fresh compilation must mutate semantic context through a single API that both
updates the live context and records an ordered typed event. The exact enum may
be split by subsystem, but its semantics are:

```rust
enum CompileContextEvent {
    RegisterModule { id: ModuleId, parent: Option<ModuleId>, name: String },
    RegisterStruct { id: StructId, owner: ModuleId, definition: StructSnapshot },
    RegisterAlias { owner: ModuleId, name: String, target: CoreType },
    RegisterMethod { id: MethodId, function: FunctionId, signature: CoreType, world: WorldRange },
    RegisterPromotionRule { lhs: CoreType, rhs: CoreType, result: CoreType },
    SetInferenceGlobal { binding: BindingId, ty: JuliaType },
    SetSpecializationPolicy { target: FunctionId, policy: Policy, reason: PolicyReason },
    SetMainScopeBinding { binding: BindingId },
}
```

Events are the mutation authority, not necessarily the final wire encoding.
Serialization compacts them into versioned sub-snapshots; restore validates and
replays those snapshots through the same context API. This prevents the live
path and restore path from having separate table-update algorithms while
avoiding an indefinitely growing append-only runtime log.

Direct writes to migrated tables become private. A migration slice is complete
only when source search and an audit show no writes outside its context API.

## Versioned snapshot envelope

The cache carries independently versioned deterministic sections:

```text
CompileContextEnvelope
  identity_graph
  struct_and_alias_state
  method_and_world_state
  inference_and_promotion_state
  runtime_policy_state
```

Each section declares a schema version and fingerprint. Section members are
sorted by stable semantic identity before serialization. Decode policy is:

1. validate the cache format and section fingerprints;
2. validate every defining semantic object;
3. build persisted-ID to current-ID relocation maps;
4. reject missing or ambiguous definitions;
5. replay sections through typed context APIs;
6. compare against lane-specific invariants in tests before executing code.

A mismatch is a cache miss/recompile where source is available, or a typed load
error where it is not. Partial hydration and best-effort interpretation are
forbidden.

## One post-hit boundary

Every cache consumer must call one production entry point:

```rust
rehydrate_after_cache_hit(program, compiled, lane, persisted_context)
```

The entry point owns validation, structural projection, snapshot replay,
thread-local semantic registry replay, and the final completeness check. During
migration it may delegate to legacy helpers, but individual lookup/load paths
must not call those helpers directly.

| Lane | Current entry/shape | Required end state |
|---|---|---|
| in-memory `PROGRAM_CACHE` | cloned final `CompiledProgram` | pass through the common completeness check; no hidden exception |
| seeded embedded program cache | decoded bytes without source context (#10335) | seed carries restore metadata or generation rejects any seed requiring context |
| persistent/embedded Base cache | `cached_base_from_serialized` plus registry/context replay | common hook replays versioned sections and Base registries |
| Base-prefix compile | cached tables seed the fresh compiler | common hook establishes the seed context before user compilation |
| preload package cache | lane-specific restored package output | common hook validates identities/world state before merge |
| `.sjvmbc` | load payload `Program`, then manual context restore | common hook restores every semantic section, including registries and scope names |
| prelude program cache | serialized lowered `Program` | remain a structural input; any compiled context derived from it uses the common hook |

No supported lane may return `compile_context: None` when reachable execution,
reflection, or specialization requires context.

## Migration plan (Issue #10462)

1. **Snapshot scoreboard — landed.** Keep `CompileContextSnapshot` exhaustive,
   deterministic, and test-only. Add every cache lane and every new semantic
   field before changing production representation.
2. **Common hook.** Route all post-hit/load sites through
   `rehydrate_after_cache_hit`, initially delegating to current restoration.
   Add a source audit that rejects direct legacy-helper calls outside the hook.
3. **Struct and alias events.** Encapsulate registration first because the
   affected bugs include constructor metadata and sibling-module identity.
   Persist/replay structured owner-aware definitions; delete matching blocks
   from `restore_compile_context_from_program`.
4. **Inference, promotion, scope, and policy — in progress.** Inference globals
   (#10333) and finalized specialization-disable flags (#10334) are persisted
   and restored exactly. Persist the promotion/scope fields still exposed by
   #10339 and replay thread-local registries from semantic data.
5. **Method/world state and relocatable IDs.** Integrate the owner-scoped ID
   migration (#10459 and its phase Issues), validate every ID-bearing bytecode
   and method-table field, and reject ambiguous relocation.
6. **Delete manual reconstruction.** Remove legacy restore guesswork and all
   scoreboard allowances. Ratchet direct semantic-table writes and cache-lane
   bypasses to zero.

Each phase is a buildable PR that deletes at least one legacy restore block or
reduces the tracked mismatch scoreboard. A phase that only adds a second path
does not reduce this debt.

## Differential test matrix

Every corpus row runs fresh plus all reachable cache lanes. Tests compare the
semantic snapshot, observable output, exception type/catchability, reflection
result, and (where applicable) GC outcome.

| Corpus | Properties that must be compared | Primary Issues |
|---|---|---|
| sibling modules with same-named structs | owner identity, parent/subtype, method dispatch | #10342, #10459 |
| module struct aliases and parametric parents | qualified/bare resolution, field/parent schema | #10336, #10337, #10341 |
| module-local functions | `methods`, `applicable`, return/effect/exception inference | #10343 |
| top-level const and mutable globals | inference-global precision and widening | #10333 |
| user `getindex`/`setindex!`/`getproperty` overrides | exact finalized disable flags after module/alias-aware method-table detection | #10334 (resolved) |
| user promotion rules | registry contents and mixed numeric dispatch | #10339 |
| required/main-scope bindings | session persistence and omitted-local filtering | #10339 |
| inner constructors and mutable structs | constructor metadata, layout, WeakRef behavior | #10092 |
| seeded program needing context | non-`None` hydrated context or generation rejection | #10335 |
| method redefinition/source world | visible method set and cache invalidation | #10462 |
| deterministic bytes across processes | stable ordering, fingerprints, relocation validation | #10051, #10462 |

#10334 is resolved by persisting the fresh method-table decisions rather than
re-discovering them from a lossy top-level IR view. Its regression corpus
includes module-owned overrides and alias-typed array receivers, and requires
exact fresh/manual/`.sjvmbc` policy equality. The scoreboard may name a known
gap while its Issue remains open. Once a row is fixed, the same PR removes the
allowance and asserts equality.

## Completion criteria

Issue #10438 owns this accepted design and the inventory above. Issue #10462
owns production completion. The implementation epic is complete only when:

- every cache hit/load reaches the common hook;
- every semantic mutation uses a typed API/event;
- versioned snapshots restore without inference from incomplete program shape;
- all persisted IDs are validated and relocated or the cache is rejected;
- the differential matrix has no unexplained differences;
- cache-on/off reflection and GC behavior match;
- the nine symptom Issues #10333–#10343 listed by #10438 are fixed or explicitly
  re-scoped with evidence; and
- legacy manual reconstruction and all temporary scoreboard allowances are gone.

Until then, those symptom Issues stay open. Closing this design Issue does not
claim the production migration has landed; it establishes the authority that
prevents each cache lane from inventing another repair path.
