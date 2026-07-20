# Semantic identities and name-string lookup retirement

Last updated: 2026-07-15

Tracks: Issue #10459. Related: #10279, #10436, #10460, #10461.

Companion document:
[`docs/vm/SEMANTIC_ID_MIGRATION.md`](./SEMANTIC_ID_MIGRATION.md) is Issue
#10459's Phase 0 deliverable — a mechanical, re-runnable inventory of every
bare-name identity site in production code (not just the six patterns
`scripts/check_name_based_lookup.sh` ratchets), classified by identity
domain / layer / migration difficulty / semantic verdict, with the as-landed
phase record and continuation Issues. This document stays the higher-level
design record (target model, vocabulary, review checklist, phase
descriptions).

## Problem

Several sjulia subsystems still use Julia display names as internal identity.
That is safe only for diagnostics and lexical lookup. It is not a semantic
identity model: two different owners can expose the same spelling, nested
`where` binders can reuse the same name, and runtime-created `TypeVar(:T)`
objects can share a printed name while remaining distinct objects.

The milestone-76 symptom backlog is now guarded by regression fixtures, but the
root representation debt remains. This document is the design record for
retiring bare string identity tables without another lookup-site exception;
see the companion document above for the current Phase 0 inventory and
migration plan.

## Identity vocabulary

Use these terms consistently:

| Term | Meaning | May be a `String`? |
|---|---|---|
| display name | User-facing spelling for `show`, diagnostics, and docs | Yes |
| lexical lookup key | Name looked up in the currently active module/scope | Temporarily yes; must resolve to an ID before semantic storage |
| semantic identity | Stable owner-scoped key used for equality, cache keys, dispatch, reflection, and serialized references | No |
| relocation key | Serialized reference that can be mapped back to the current session's semantic identity table | No, except as display metadata |

The invariant is: after a name has been resolved, semantic decisions use an ID
or a structured type graph, not the display string.

For `where` clauses, owner-scoped IDs are allocated and made visible through the
shared lexical environment described in
[WHERE_BINDER_ENVIRONMENT.md](./WHERE_BINDER_ENVIRONMENT.md) (Issue #10436).
This document owns the identity model; the binder-environment doc owns the
scoping and allocation flow that feeds it.

## Current identity-bearing inventory

### Existing typed/scoped identity pieces

- `ConcreteTypeId` / `ConcreteTypeKey` / `TypeInternTable`
  (`subset_julia_vm_bytecode`) already provide session-scoped dispatch type
  identities for call-site caching. See `docs/vm/TYPE_INTERNING.md`.
- `CoreTypeVar::scope_id` and `CoreTypeVar::rigid_identity`
  (`subset_julia_vm_types/src/inference_core/type_core/repr.rs`) distinguish
  scoped binders from rigid free runtime TypeVars inside the structured
  `CoreType` path.
- `JuliaType::RuntimeTypeVar { id, ... }` and `RuntimeTypeVarValue { id, ... }`
  preserve object identity for runtime-created `TypeVar` values.
- `TypeVarScope` in `type_core/match.rs` now stores binders by a compound
  `(scope_id, name)` key for exact retrieval, while retaining a by-name stack
  only as a lexical lookup aid.

### Remaining string-keyed semantic debt

These are the current guarded counts on `origin/main` as of 2026-07-15:

| Bucket | Count | Why it is debt | Guard |
|---|---:|---|---|
| `HashMap<String, CoreTypeVar>` in `inference_core` | 0 | Fully retired for the grep shape; do not reintroduce | `scripts/check_name_based_lookup.sh` |
| `HashMap<String, CoreType>` in TypeVar/core binding paths | 13 | Same-name binders can collide unless callers prove the map is only a lexical scratchpad. Constructor/registered-parent `UnionAll` substitution now carries `CoreTypeSubstitution { variable: CoreTypeVar, value }`, so capture avoidance uses `CoreTypeVarId` whenever available and falls back to names only for unresolved lexical leaves (#10436). | `scripts/check_name_based_lookup.sh` |
| `HashMap<String, StructInfo>` in compile struct tables | 0 | Retired by `StructRegistry` (#11078); do not reintroduce a parallel name-keyed layout table | `scripts/check_name_based_lookup.sh` |
| Bare `struct_table.get(name/base_name)` lookups | 0 | Retired by the owner/current-module/Main/lexical ordering in `StructRegistry::resolve_scoped` (#11046); inference-only `StructTypeInfo` layout projection is classified behind `lookup_struct_type_info` | `scripts/check_name_based_lookup.sh` |
| `runtime_typevar_identities: HashMap<(String, Option<String>), ...>` | 0 | Retired; do not reintroduce a VM-global name/bound identity table | `scripts/check_name_based_lookup.sh` |
| `runtime_typevar_projection_identities: HashMap<TypeVarProjectionKey, RuntimeTypeVarValue>` | 0 | Retired (Issue #10987): the key is fully structural — `owner: CoreType` (normalized final body, nested `UnionAll` binders PRESERVED so a binder occurring only inside a nested binder's bound still distinguishes wrappers), `binder_depth: usize` (from the final body, separating nested same-name binders across suffix views), and the as-declared `declared_lower`/`declared_upper` bounds as PARSED `JuliaType`s (not `CoreType` — its bridge strips module qualification, which would collapse `T<:M1.Box{Int}` with `T<:M2.Box{Int}`). The bounds must stay in the key (the body-derived owner does not encode binder bounds under the legacy string-shaped `UnionAll` representation, so same-body wrappers with different bounds are distinct binder objects — upstream gives each `where` its own TypeVar); the rendered name/bound strings no longer participate — the display name is non-key metadata on the stored `RuntimeTypeVarValue` | owner normalization + external-ID / inner-first shadow-depth / binder-rename / bound-spelling / distinct-bounds tests + fixture `types/typevar_projection_structural_key_10987.jl`; #10459 |

When a row is structurally retired, lower the matching baseline in
`scripts/check_name_based_lookup.sh` in the same PR. When adding a temporary
string-keyed table, add it only as a lexical lookup cache and document why it
cannot affect semantic equality/dispatch/reflection.

## Target model

The long-term model is a small set of typed IDs allocated from ownership, not
from display text:

```rust
struct ModuleId(u32);
struct TypeVarId {
    owner: TypeOwnerId,
    index: u32,
}
struct StructId {
    module: ModuleId,
    local: u32,
}
struct FunctionId {
    module: ModuleId,
    local: u32,
}
struct MethodId {
    function: FunctionId,
    local: u32,
}
```

Exact storage can be interned IDs or relocatable stable keys. The required
properties are:

1. IDs are allocated by lexical/module ownership, never by `HashMap` iteration
   order.
2. Serialized identities use explicit relocation tables (Pattern B); runtime
   state that never crosses the wire is rebuilt deterministically on every
   lane (Pattern A).
3. Qualified and bare name lookup resolve to IDs; the name is not the identity.
4. `JuliaType`, `CoreType`, method signatures, and runtime reflection may keep a
   display name, but equality and cache keys retain the semantic ID.
5. Diagnostics project IDs back to module-qualified user-facing names.

These structs are architectural vocabulary, not a mandate to introduce an ID
that no production consumer reads. A phase may satisfy the properties with an
existing canonical index or a structural resolver; an unread parallel ID is
itself debt, not progress.

## Migration phases

### Phase 0 — documentation and ratchets

Status: **done**. This document (target model, vocabulary, review checklist)
plus `scripts/check_name_based_lookup.sh` (the six-pattern ratchet gate) plus,
as of Issue #10459's own Phase 0 deliverable,
[`docs/vm/SEMANTIC_ID_MIGRATION.md`](./SEMANTIC_ID_MIGRATION.md) — a
mechanical, re-runnable inventory (`scripts/semantic_id_inventory.sh`,
committed snapshot `docs/vm/SEMANTIC_ID_INVENTORY.tsv`) covering every
bare-name identity site in production code, not just the six ratcheted
patterns, classified by identity domain / layer / migration difficulty /
semantic verdict, with an as-landed phase table and continuation Issues. Read
that document for current headline counts and the Phase
2a/2b/3/cache-lane/4 verdict table; this section keeps only the phase
*descriptions*.

- Keep an explicit inventory of string-keyed semantic debt.
- Prevent new name-only identity sites from landing silently.
- Tighten ratchet baselines whenever a bucket shrinks.

### Phase 1 — TypeVar vertical slice

Status: **complete**, including the residual closed by Issue #10987 (Phase 1
completion). Constructed runtime TypeVars retain their ID structurally;
dependent bounds reuse those IDs (`B.ub === A`, `C.ub === B`); UnionAll
projection caches use a structural `CoreType` owner. There is no VM-global
`(name, upper-name)` identity cache. Wrapper application promotes
identity-bearing chains to `RuntimeUnionAll`, so binder, bound, and body
substitution remain aligned.

- Introduce a typed `TypeVarId` wrapper around the current
  `scope_id`/`rigid_identity` split. Initial slice landed as
  `CoreTypeVarId`, a typed projection over the legacy serialized fields, and
  `TypeVarScope` / `TypeVarBindingState` now key through that projection rather
  than rebuilding raw `(scope_id, rigid_identity)` tuples.
- Allocate IDs at binder lowering, including nested same-name binders.
- Carry `TypeVarId` through `CoreType::TypeVar`,
  `JuliaType::RuntimeTypeVar`, bound resolution, subtype/typejoin,
  reflection, and cache serialization.
- Keep `runtime_typevar_projection_identities` keyed by the fully structural
  `TypeVarProjectionKey { owner, binder_depth, declared_lower,
  declared_upper }` (Issue #10987 replaced the
  `(CoreType, usize, String, Option<String>)` key, closing the one residual
  this phase had left): the rendered NAME is non-key display metadata on the
  stored `RuntimeTypeVarValue` (a rename-spelled lookup of the same position
  used to miss the cache and mint a second, wrongly-distinct identity), and
  the as-declared bounds participate as PARSED structural `CoreType`s rather
  than rendered strings (insensitive to `Int`-vs-`Int64` spelling and
  interval-format reconstruction). The bounds must remain key components:
  the owner is derived from the wrapper's final body, which under the legacy
  string-shaped `UnionAll` representation does not encode binder bounds —
  distinct same-body wrappers (`Vector{Int64} where Int64>:Signed` vs
  `Vector{Int64} where Signed<:Int64<:Real`) are distinct binder objects
  upstream. Structured bodies that would let the owner carry this are Issue
  #10460's scope. Preserve external free-TypeVar IDs in the owner key and
  never reintroduce a rendered-name-only identity cache.

### Phase 2a — `ModuleId` foundation (module/global scoping)

Status: **foundation and one persisted consumer landed; the remaining named
tables were adjudicated** (Issue #10988, PRs #11033/#11084/#11191).
Added by the Phase 0 inventory, not present in the epic's original prose:
`StructId { module: ModuleId, ... }` and `FunctionId { module: ModuleId, ... }`
both embed a `ModuleId`, so it must exist and be allocated before Phase 2b/3
can type their IDs correctly.

**Landed**:

- `ModuleId(u32)` + `ModuleInternTable` (`subset_julia_vm_bytecode::module_intern`),
  allocated at module *registration order* — a deterministic depth-first walk
  of the `Program`'s module tree (`compile/collect.rs::register_module_ids`,
  mirroring `collect_module_info`'s own recursion) — never from `HashMap`
  iteration order.
- `RuntimeCompileContext::module_registry` (Pattern A: derived fresh every
  compile from the module AST, not itself serialized — see
  `docs/vm/CACHE_ARCHITECTURE.md`'s "Owner-scoped id relocation pattern").
- `CompiledProgram::macro_bindings` re-keyed `String` -> `ModuleId`, with a
  sibling `CompiledProgram::module_registry: ModuleInternTable` serialized
  alongside it (Pattern B: a real persisted relocation table; `CACHE_VERSION`
  136 -> 137). This is the one *identity-bearing, genuinely wire-serialized*
  module-domain table this phase migrated end-to-end, chosen over the 12
  tables named in the issue body because none of those are fields of any
  struct that is itself bincode-serialized — they are transient
  `CorePipeline`/`CoreCompiler`/`SharedCompileContext` compile-pipeline state
  (one, `inference_global_types`, is later cloned into the `#[serde(skip)]`
  `RuntimeCompileContext`, but never crosses the wire even then) — see the PR
  body / `docs/vm/SEMANTIC_ID_MIGRATION.md`'s Phase 2a row for the precise
  per-table ownership and the full reasoning.
- Same-name-different-module regression tests, both fresh compile and cache
  restore (`compile/cache.rs::same_name_different_module_gets_distinct_and_stable_ids_issue_10988`,
  and the extended `restored_compile_context_matches_fresh_compile_10265`/
  `compile_context_snapshot_restore_lane_scoreboard_10462` parity guards).

**As-landed verdict for the 12 originally named tables** (PR #11191):
`module_functions`/`module_exports`/`module_constants`/`module_abstract_names`/
`module_imported_bindings`/`module_struct_names`/`module_aliases`/`module_usings`,
`global_types`/`inference_global_types`/`global_struct_names`/`global_const_structs`
— remain `HashMap<String, _>`-keyed because their keys are either canonical
qualified paths / compound qualified bindings (sanctioned lexical
boundaries) or verified-inert inference bookkeeping. `module_aliases` is a
lexical import boundary; its actual source-order defect was fixed as #11176.
The `semantic_id_inventory.py` verdict rules make this judgment
machine-readable. Re-keying these tables would swap representation without
retiring a semantic identity decision.

### Phase 2b — module-owned struct identity

Depends on Phase 2a.

Status: **`StructId`/`StructRegistry` re-key and owner-scoped resolution are
complete using Pattern A** (Issues #11078/#11046, PRs #11156/#11413).

- `StructRegistry` stores layouts by owner-scoped `StructId`; its `by_name`
  map is the single lexical name-to-ID boundary. The old
  `HashMap<String, StructInfo>` surface is zero.
- `StructInfo` derives no `Serialize` and `RuntimeCompileContext` is
  `#[serde(skip)]`, so the registry is rebuilt on fresh and restore lanes.
  The earlier relocation-table premise is **REFUTED (PR #11156)**; Pattern A,
  not Pattern B, is authoritative.
- `StructRegistry::resolve_scoped` owns exact-qualified, current-module,
  Main/Base-origin, and lexical-alias ordering. The 19 bare resolver decisions
  and parallel Base recovery tables are retired; both struct audit counts are
  zero and mutation tests pin Main/cache-restore delegation.
- Same-owner/different-owner comparison and dispatch regressions are guarded by
  #11021/#11076/#11094 coverage. Historical investigation details remain in
  `SEMANTIC_ID_MIGRATION.md`, clearly labeled as superseded where appropriate.

### Phase 3 — function and method identity

Depends on the Phase 2a owner vocabulary. Phase 2b has landed, but the
function/method continuation is judged by its own consumers.

Status: **unused `FunctionId`/`MethodId` carriers were deliberately not
introduced; #11095 owns the identity-bearing resolver/table residual**
(Issue #10990, PR #11098). The investigation found that the
function/method-sig domain has no bounded, self-contained table a
`FunctionId` could retire in one PR. `function_indices`/
`source_ordered_method_sigs`/`method_tables`/`imported_functions`
(`subset_julia_vm_compile/src/compile/{context,pipeline_ctx}.rs`) are all
`CorePipeline`/`SharedCompileContext`-transient — none is a field of any
struct that is itself bincode-serialized. The one genuinely serialized
function-domain field, `CompiledProgram::functions: Vec<Rc<FunctionInfo>>`,
is already index-keyed (`global_index`), so a `FunctionId` there would
formalize an existing owner derivation, not retire a bare-name `HashMap`.
Building the type anyway would have been exactly the "parallel path" the
target model forbids. See `docs/vm/SEMANTIC_ID_MIGRATION.md`'s
"Phase 3 as-landed judgment" section for the full investigation writeup.

**What actually landed**: a fix for Issue #11088 (same-named functions in
sibling modules wrongly comparing `==`/`===` and sharing a `typeof`) that
needed no `FunctionId` — `emit_function_value_named`
(`subset_julia_vm_compile/src/compile/core_compiler.rs`) always used the bare
declared name as a resolved function value's runtime type identity (a fix
originally scoped to Issue #10077's bare-vs-qualified-SAME-declaration case,
over-applied to also collapse different declarations sharing a bare name);
now uses the qualified spelling instead whenever another module's qualified
`method_tables` key answers to the same bare name — gated by a
`unique_using_owner` helper (export-aware via `module_exports`, added after
two rounds of adversarial review caught it wrongly diverging a genuinely
`using`d declaration's own bare-vs-qualified identity) so an unrelated
sibling never flips the identity of a declaration actually in scope.
Composes correctly with
Issue #11021's owner-aware struct comparison fix (already landed) for the
`typeof(f1) === typeof(f2)` case. Also found and filed separately: Issue
#11089 (bare-name method-table visibility not scoped by which module is
actually `using`'d — higher blast radius, not fixed).

The remaining work is behavioral, not ID-shaped by assumption: #11095 must
retire identity-bearing `function_indices`/`source_ordered_method_sigs`/
`method_tables` and using-scope visibility decisions through one resolver.
It may add an ID only when that resolver or a runtime consumer reads it.
Same-named function identity comparison is already fixed by #11088.

### Cache lane (cross-cutting, parallel with 2a/2b/3)

Not a sequential phase. Pattern B (explicit relocation + `CACHE_VERSION`)
applies only when the identity-bearing field itself crosses a wire, as
`CompiledProgram::macro_bindings` does. Pattern A applies to state rebuilt on
fresh and restore lanes, as `StructRegistry` does. File proximity to
`compile/cache.rs` is not evidence of serialization.

### Phase 4 — remove compatibility fallbacks

Issue #10992 consumes the landed verdicts rather than waiting for unused ID
types to exist.

- [ ] Every `identity-bearing` inventory residual is retired or linked to its
      owning continuation Issue.
- [x] Every `lexical-boundary` / `inert` downgrade has an explicit rule backed
      by landed evidence (Issue #11284).
- [x] Unclassified `typevar_core_bindings` reached zero. The only raw map
      declarations are the exact private `LexicalTypeBindings` single-match
      substitution authority and `RenderedTypeParseCache` pure parser memo;
      both are explicit lexical-boundary rules and audit anchors (#10992).
- [x] `struct_table_bare_gets_compile` reached zero through #11046's
      owner/current-module/Main/lexical resolver; the parallel Base fallback
      table was removed.
- [ ] Function/method identity-bearing sites are retired through #11095.
- [ ] The stabilized identity-bearing inventory is promoted into a failing
      audit before #10459 closes.

## Review checklist

For any PR touching type identity, method signatures, struct lookup, runtime
reflection, or cache serialization:

1. Is the string used only for display or lexical lookup?
2. If a semantic lookup remains keyed by `String`, is there an Issue-linked
   reason and a ratchet baseline?
3. Does the change preserve same-spelling/different-owner identity?
4. Does the fresh compile path agree with cache restore?
5. Did `bash scripts/check_name_based_lookup.sh` pass, and was any lowered
   baseline tightened in the same PR?
