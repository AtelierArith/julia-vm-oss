# Interned Concrete Type IDs (`ConcreteTypeId`) — Design Record

*Created: 2026-07-06 (Issue #9197, slice 1 — the intern-registry foundation).*

This is the design record for the **session-scoped concrete-type interning
registry** that Issue #9197 (`[arch] 実行時型同一性を文字列型名/未検証ハッシュから
interned 型 ID + typemap 型索引へ`) makes the single source of runtime type
identity. It is a per-topic design record in the style of
[CONCRETETYPE_RETIREMENT.md](./CONCRETETYPE_RETIREMENT.md),
[LATTICE_TYPE.md](./LATTICE_TYPE.md), and
[CACHE_ARCHITECTURE.md](./CACHE_ARCHITECTURE.md); read
[TYPE_SYSTEM.md](./TYPE_SYSTEM.md) §"Type Representations" and
[TYPE_REPRESENTATIONS.md](./TYPE_REPRESENTATIONS.md) first for how `CoreType`,
`JuliaType`, `ValueType`, and the struct-table `type_id` relate today.

> **Why a new file rather than extending TYPE_SYSTEM.md.** TYPE_SYSTEM.md is the
> stable overview of the *four existing* representations and their conversions;
> TYPE_REPRESENTATIONS.md is the exhaustive conversion inventory. `ConcreteTypeId`
> is a *fifth, runtime-only* identity layer introduced by a multi-slice epic with
> its own invalidation, REPL-boundary, and per-slice-consumer contracts. Those
> belong in a dedicated design record so the epic has one home; TYPE_SYSTEM.md and
> the docs table in `AGENTS.md` cross-link here instead of absorbing it.

## Status

| Slice | Content | State |
|-------|---------|-------|
| **S1** | `ConcreteTypeId(u32)` intern registry + API + unit tests, **UNWIRED** | design + module landed; no production call site |
| **S2** | L1 call-site inline cache keyed by `SmallVec<[ConcreteTypeId; 4]>` (exact-match, replaces the unverified u64 hash) | **landed** (Issue #9197 S2) — see "Slice 2 deliverable" below |
| S3 | L2 dispatch cache keyed by interned id sequence + bounded overflow eviction | **landed** (Issue #9197 S3) — see "Slice 3 deliverable" below. **Untracked-kind skip regression fixed by #9427** (see below) |
| S4 | `parametric_type_args` / `split_parametric_args` runtime string-parse retirement → structured id access | **landed** (Issue #9197 S4) — see "Slice 4 deliverable" below (+ left-for-S5 list) |
| S5 | `FirstArgIndex` → typemap. **Landed:** the primitive-complete first-arg gather (buckets every method by first-param nominal family; a sealed-primitive argument gathers its own + abstract-supertype + wildcard buckets, dropping every struct/container method it can never match) + the earlier untracked-kind re-caching (#9427). **Deferred to S6/S7:** the struct/abstract-argument gather (needs the abstract-container supertype relation) and the `vm/dispatch.rs` runtime type-name parsers on that resolve path | **partially landed** (Issue #9197 S5) — see "Slice 5 deliverable" below; struct-argument typemap + string-parse retirement deferred |
| S6 | Precise invalidation: replace `note_method_table_mutation`'s whole-clear with per-name backedge invalidation of the runtime dispatch caches | **landed** (Issue #9197 S6) — see "Slice 6 deliverable" below; finer signature-intersection precision deferred to S7 |
| S7 | Cache-boundary method-table key → typed `MethodTableKey` (retire the last bare-`String` key at the Base-cache boundary) + deterministic serialization. **Deferred (still #9199-gated):** the REPL `struct_name → type_id` re-resolution retirement, which needs #9199 to persist the intern registry across evals | **landed** (Issue #9197 S7) — see "Slice 7 deliverable" below |

The registry module (`subset_julia_vm_bytecode/src/type_intern.rs`) carried
a module-level `#![allow(dead_code)]` while S1 was unwired. **S2 removed it** (the
`intern` / `intern_primitive` / `intern_struct` API and every `ConcreteTypeKey`
variant now have a production consumer); the read/render/probe surface still
reserved for S3–S7 (`lookup`, `key`, `display_name` + `render_*`, `len`,
`is_empty`, `ConcreteTypeId::index`, `intern_struct`) carries a **narrow
per-item** `#[allow(dead_code)]` documenting which later slice consumes it.

## The problem this fixes

Runtime type identity in sjulia is currently represented by **type-name strings
and unverified hashes**, on which the last three days of dispatch work
(#9108/#9111/#9112/#9113/#8603) were all built. The concrete facts (all quoted
from the Issue #9197 investigation, verified in-tree):

1. **L1 inline cache is an unverified `u64` hash match.**
   `hash_call_site_fingerprint` (`vm/mod.rs:374`) hashes arg count + per-arg type
   tags into one `u64`; `lookup_call_site_inline_cache` (`vm/state.rs:1701`) hits
   on `u64` equality alone and **never re-checks the actual argument signature**.
   Correctness is therefore probabilistic (collision ≈ 2⁻⁶⁴) — a shape upstream
   Julia does not have.
2. **L2 dispatch keys are runtime-derived type-name strings**
   (`vm/exec/call_dynamic.rs`): every L1 miss calls `get_type_name(arg)` →
   `dynamic_dispatch_type_name` → sequential hash.
3. **Parametric types are string-parsed at runtime.** `parametric_type_args` /
   `split_parametric_args` (`vm/mod.rs:391`) re-parse names like `"Complex{Int64}"`
   with `find('{')` + top-level-comma splitting on every consultation.
4. **`FirstArgIndex` buckets primitives only** (`runtime_types/method_table.rs`,
   `concrete_key_for` returns a key only for `CoreType::Primitive`); struct /
   abstract / named first-args all fall to `wildcard` and are linearly scanned,
   because `Vector{Float64} <: Array` is a subtype relation a name bucket can't see.
5. **Type identity is double-managed.** A struct value carries both `type_id:
   usize` and `struct_name: Rc<str>` (#9125); #9167 produced a value whose
   `struct_name`/`type_id` said `Complex{Int64}` while the fields were `F64` — a
   tag/payload mismatch.
6. **Base-cache and REPL boundaries are string-keyed**
   (`HashMap<String, MethodTable>` in `compile/precompile.rs`; the REPL re-resolves
   `struct_name → type_id` every eval, `repl/session.rs:235`).

### The headline counterexample: `type_id` does not identify a concrete type

The single sharpest piece of evidence lives in the doc comment on
`hash_struct_dispatch_identity` (`subset_julia_vm_vm/src/vm/mod.rs:~215`):

> `type_id` must NOT be used here — `NewStruct` refines an instance's
> `struct_name` from runtime field values (`resolve_any_type_params_from_values`)
> while keeping the definition's `type_id`, so one `type_id` can carry many
> dispatch names (e.g. `SubArray{Int64, 1}` vs `SubArray{Float64, 2}`; caught by
> the `subarray_map_over_view_5137` fixture).

So the L1 fingerprint is forced to hash the *name string* (`struct_name`) rather
than the numeric `type_id`, because **`type_id` conflates
`SubArray{Int64,1}` with `SubArray{Float64,2}`** — the compiler-struct-table
index is a *definition* index, not a *concrete-type* index. The whole point of
`ConcreteTypeId` is to be the identity `type_id` is not: **distinct type
parameters ⇒ distinct id.** That is the property the registry is built around,
and the property the S1 unit tests pin (`param_distinct_subarray`).

## Design: `ConcreteTypeId(u32)` + a structural intern key

A **session-scoped** registry hands each *concrete* type a stable, dense
`ConcreteTypeId(u32)`. Two type spellings map to the same id **iff** they are the
same concrete type *including all type parameters*.

### The intern key includes full type parameters, by child id

The intern key is **structural and recursive**: nested parametric types reference
their parameters by `ConcreteTypeId`, so the registry is a DAG and equality /
hashing is by interned id — never by re-parsing a rendered name (retiring current
fact 3). Modeled as `ConcreteTypeKey`:

- `Primitive(name)` — sealed primitives (`Int64`, `Float64`, `Bool`, `String`, …).
  Sealed ⇒ exact-name identity is correct (mirrors `FirstArgIndex`'s
  primitive-only bucket soundness argument, current fact 4).
- `Struct { name, params: Vec<ConcreteTypeId> }` — a nominal type with **fully
  resolved** parameters. `SubArray{Int64,1}` and `SubArray{Float64,2}` differ in
  `params` ⇒ **distinct ids**, exactly the conflation `type_id` cannot express.
- `IntValue(i64)` — a **value type-parameter** (the `N` in `Array{T,N}`, the
  dimensionality in `SubArray{Int64,1}`, `Val{x}`). Interned like any other type
  so a `Struct`'s `params` stay uniformly `Vec<ConcreteTypeId>`; this mirrors
  upstream, where value params are ordinary `jl_value_t*` entries in the
  `lookup_type` key (`julia/src/jltypes.c`).
- `Tuple(Vec<ConcreteTypeId>)` — `Tuple{T1, …}`, element ids in order.
- `NamedTuple { names, params }` — `NamedTuple{(:a, :b), Tuple{…}}`.
- `Array { element, ndims }` — the `Array{T,N}` wrapper (`Vector`/`Matrix`/`Array`),
  mirroring the `(element_type, ndims)` identity `hash_struct_dispatch_identity`
  already uses for Memory-backed wrappers.
- `Memory { element }` — `Memory{T}`.
- `Range { element, step, is_float, is_step }` — the visible range type
  parameters (`UnitRange{T}` / `StepRange{T,S}`) plus the native range shape flags
  used by the exact runtime call-site key.
- `Enum(name)` — a `@enum` type name.
- `Opaque(name)` — **added by Issue #9427** for value kinds whose dispatch
  identity is a single opaque type-name string not (yet) decomposed
  structurally: the `Type{T}` type-object of a `DataType`, function / closure /
  composed-function callable singletons (`typeof(f)` / `ComposedFunction`), and
  the nominal singletons `Module` / `IOBuffer` / `Base.Generator` / `TypeVar` /
  RNGs / macro-AST nodes. `name` is exactly the pre-#9404
  `get_type_name` / `dynamic_dispatch_type_name` string, so the interned-id
  partition equals the retired L2 string-key partition, and — being a distinct
  variant — an `Opaque("Foo")` never collides with a same-spelling `Struct` /
  `Enum` / `Primitive`. See "Untracked-kind re-caching" below.

The S1 variant set was intentionally the same identity surface that
`hash_call_site_value_tag` / `hash_struct_dispatch_identity` fold into the L1
fingerprint today (`vm/mod.rs:222`, `:267`), so S2 could replace the *hash* with
an *exact id sequence* without changing which value kinds participate. Value
kinds whose dispatch identity was not yet tracked (closures, generators, IO,
`Type{T}`, …) originally had no key and skipped L1/L2 — **which S3 turned into a
6× dispatch regression (Issue #9427); `Opaque` closes that gap for all but a few
cold carriers (see below).**

### Data structures and API

```text
struct TypeInternTable {
    keys:  Vec<ConcreteTypeKey>,                      // id.0 as usize → key  (round-trip / display)
    index: HashMap<ConcreteTypeKey, ConcreteTypeId>,  // key → id  (dedup)
}
```

- `intern(&mut self, key) -> ConcreteTypeId` — idempotent; first sight assigns the
  next dense id, re-interning returns the same id (the append gives stability).
- `lookup(&self, &key) -> Option<ConcreteTypeId>` — read-only probe.
- `key(&self, id) -> Option<&ConcreteTypeKey>` — id → structural key.
- `display_name(&self, id) -> Option<String>` — recursively renders the canonical
  Julia spelling (`SubArray{Int64, 1}`, `Array{Complex{Int64}, 1}`, …) by walking
  child ids; the key → id → key(structural) → display(string) round-trip is the
  demonstration that no information is lost vs. the old name strings.
- convenience builders (`intern_primitive`, `intern_struct`) so consumers and
  tests read cleanly.

Ids are assigned in first-seen order and never reused within a session, so an id
handed out early stays valid for the whole session (S1 test
`id_stability_within_session`). Because the id space is `u32`, `intern` guards the
`usize → u32` narrowing; a session realistically holds thousands of concrete
types, far below `u32::MAX`.

## Relationship to the existing representations

`ConcreteTypeId` is a **runtime identity handle**, not a replacement for any
existing representation:

- **struct-table `type_id: usize`** is a *compiler struct-table index* — a
  definition index that is (a) parameter-blind (the SubArray conflation) and (b)
  "sometimes `0` / resolve-later" at many sites (see CONCRETETYPE_RETIREMENT.md §1).
  `ConcreteTypeId` is the identity `type_id` was mis-serving as. The struct table
  stays as the definition/layout store (`StructDefInfo`,
  `subset_julia_vm_bytecode/src/metadata.rs`); a `ConcreteTypeId` for a struct
  key can *carry* the definition's `type_id` alongside its resolved `params`, but
  the id — not `type_id` — is what dispatch keys compare.
- **`CoreType`** (`subset_julia_vm_types`) is the *shared semantic core* and the
  serialized source of truth for method signatures (`MethodSig.core_signature`,
  #6336). Per CONCRETETYPE_RETIREMENT.md §1 the decision is **keep `CoreType`
  pure** — do not add `type_id`/VM artifacts to it. `ConcreteTypeId` therefore
  lives *outside* `CoreType`: a `CoreType::Struct { name, params }` that is fully
  concrete can be *projected into* a `ConcreteTypeKey` and interned, but the
  registry does not mutate or embed itself in `CoreType`. The registry is to
  runtime dispatch what `CoreType` is to compile-time signatures.
- **`JuliaType`** is the user-facing spelling (`typeof`, errors, field types); its
  known weakness is that parametric user types are an *opaque string*
  (`JuliaType::Struct("Complex{Float64}")`, TYPE_REPRESENTATIONS.md §1.1).
  `display_name(id)` produces the same spelling *from structure*, so id → string
  is available where a name is still needed without the string being the identity.
- **`struct_name: Rc<str>`** on `StructInstance` stays for now; the registry is the
  path by which the #9125 "eliminate the redundant-with-`type_id` name field"
  follow-up eventually becomes "carry one `ConcreteTypeId` instead of
  (`type_id`, `struct_name`)".

## Single-threaded VM policy

Per [SINGLE_THREADED_VM.md](./SINGLE_THREADED_VM.md) a `Vm`/`REPLSession` has one
host owner and hot values use `Rc`/`RefCell`/`thread_local!`. The registry is
**VM-session-local state** and follows that policy:

- It is a plain owned struct (no `Arc`/`Mutex`/atomics); when wired it becomes a
  `Vm` field (or a `thread_local!`/`Rc<RefCell<…>>` if a consumer needs shared
  interior mutability). `!Send`/`!Sync` is fine and expected.
- Keys use `Box<str>`/`Vec` today for a self-contained module; wiring may switch
  struct-name storage to the already-interned `Rc<str>` `struct_name` so interning
  a struct key is a refcount bump, not a re-allocation.

## Invalidation story

Ids are **append-only and stable within a session** — interning never
invalidates an existing id (a new concrete type gets a *new* id; existing ids keep
pointing at the same key). What must invalidate on a method-table mutation or an
`eval`-defined new type is the **dispatch caches keyed by ids**, not the id
registry itself.

Today method-table mutation calls `note_method_table_mutation`
(`vm/state.rs:~1615`) which bumps `dispatch_generation` and **whole-clears every
decision HashMap** (coarse v1). Under interning:

- New concrete types encountered after a mutation are simply interned with fresh
  ids — no existing entry changes.
- The dispatch caches (S2/S3) tag entries with the generation exactly as
  `CallSiteCache` does now (`generation` field, `vm/mod.rs:526`); a generation
  bump makes stale entries miss without touching the registry.
- **S6 (landed)** replaces the whole-clear with per-name precise invalidation
  (`note_method_table_mutation_for`, `vm/state.rs`): only cache entries whose
  selected method belongs to the mutated generic function (or the builtin
  fallback) are dropped, and the L1 generation is **not** bumped so unrelated
  inline-cache slots stay live. See "Slice 6 deliverable" below. The registry is
  untouched by any of this — it is a monotonically growing id ↔ key map for the
  session's lifetime.

Forward reference: S6 is where the backedge graph, world-age visibility
(`current_world`/frame `world_age`/`Function.min_world`, `vm/state.rs:~1571`), and
the id-keyed caches meet.

## The REPL boundary

The REPL re-resolves `struct_name → type_id` on **every** eval because "VM's
`type_id` may not match compile-time struct_table indices"
(`repl/session.rs:~235`). This is a direct symptom of `type_id` being a
per-compilation index rather than a session-stable identity.

`ConcreteTypeId` is the session-stable identity that makes this re-resolution
unnecessary: a concrete type interned in eval *N* keeps its id in eval *N+1*, so
persisted globals/struct values can carry an id instead of a name to re-resolve.
**S1 designs this boundary; it does not implement it.** Full retirement of the
re-resolution depends on **#9199** (persisting the registry across REPL evals as
part of session state). Until then the registry is constructed per-VM and the
REPL boundary stays as-is; the contract is only that ids are stable *within* one
VM/session so a future #9199 can make them stable *across* evals by persisting the
one table.

## Consumers per slice (the contract later slices code against)

| Slice | Consumer | Contract on the registry |
|-------|----------|--------------------------|
| S2 | L1 call-site inline cache | key = `SmallVec<[ConcreteTypeId; 4]>` of the arg ids; **hit = exact id-sequence equality** (replaces the unverified `u64` in `CallSiteCache`, `vm/mod.rs:524`). The registry supplies one id per arg via the same value-kind coverage as `hash_call_site_value_tag`; untracked kinds → no id → skip L1 (unchanged policy). |
| S3 | L2 dispatch cache | key = interned id sequence instead of the sequential hash of `get_type_name` strings (`vm/exec/call_dynamic.rs`); `HashMap` does its own hashing of the id slice. |
| S4 | `parametric_type_args` retirement | `TypeInternTable::intern_type_name` is the canonical rendered-name → structured `ConcreteTypeKey` DAG decomposition (parse once, then identity by id). The `vm/mod.rs` `parametric_type_args`/`split_parametric_args` string parsers are **deleted** (their only live consumer, the display-path show-method supertype walk, no longer substitutes parameters). See "Slice 4 deliverable" below. |
| S5 | `FirstArgIndex` → typemap | insertion resolves subtype relations and buckets by id (incl. struct/abstract first-args), a reduced form of upstream typemap kind-splitting; retires the `Primitive`-only bucketing (`method_table.rs`, `concrete_key_for`). |
| S6 | cache invalidation | id-keyed caches drop only the entries whose resolved method belongs to the mutated generic function (per-name backedge, `dispatch_decision_affected`); registry stays append-only. |
| S7 | Cache boundary | the serialized Base-cache method-table map is keyed by the typed `MethodTableKey` (not a bare `String`) and its section serializes deterministically (sorted). The REPL `struct_name → type_id` re-resolution retirement stays #9199-gated (needs the registry persisted across evals). |

S2 (L1 exact-match) is the first consumer and should code against `intern` +
exact id-sequence keys.

## Upstream Julia shape (`./julia`, verified against the in-tree checkout, 1.12.6)

The design mirrors upstream's "types are process-interned objects; caches key on
the interned object, not a hash":

- **Per-typename type cache** — `jl_typename_t` holds `cache` (sorted) +
  `linearcache` (unsorted) svecs (`julia/src/julia.h:533-534`); `lookup_type`
  (`julia/src/jltypes.c:1112`, via `lookup_type_set`/`lookup_type_idx_linear`,
  `:1030`/`:1078`) canonicalizes so **the same parameters always yield the same
  `jl_datatype_t*` pointer.** `ConcreteTypeId` is the sjulia analogue of that
  canonical pointer; the intern *key including full parameters* is the analogue of
  `lookup_type`'s `(typename, iparams…)` key.
- **Fastest cache still verifies** — the 4-way `call_cache[N_CALL_CACHE]`
  (`julia/src/gf.c:4056`) validates a hit with `sig_match_fast`
  (`julia/src/gf.c:4036`), which checks each argument's `jl_typeof` against the
  signature by **pointer identity**. This is the exact contrast with sjulia's
  current unverified L1 `u64` match, and the target shape for S2's exact
  id-sequence compare.
- **TypeMap tree** — `jl_typemap_level_t` (`julia/src/julia.h:867-882`) splits on
  `targ` (`Type{LeafType}`) → `arg1` (leaf concrete type) → `name1`/`tname`
  (TypeName parent chain, up to/excluding Any) → `linear` → `any`, walked by
  `jl_typemap_level_assoc_exact` (`julia/src/typemap.c:1175`). The parent-chain
  split is upstream's structural answer to "a name bucket can't see
  `Vector{Float64} <: Array`", and the reference shape for S5.

## Slice 1 deliverable (this PR)

- This design record.
- `subset_julia_vm_bytecode/src/type_intern.rs`: `ConcreteTypeId`,
  `ConcreteTypeKey`, `TypeInternTable` (`intern`/`lookup`/`key`/`display_name` +
  builders), **UNWIRED** (`#![allow(dead_code)]`, no production call site — dispatch
  and compile paths are byte-for-byte unchanged).
- Unit tests: parameter-distinct interning (the SubArray example), idempotent
  re-interning, nested parametric types, id stability within a session, display
  round-trip, and lookup-before-intern.

No fixture changes; no full-suite run required (the module is unwired). S2 wires
the L1 cache and removes the `dead_code` allowance.

## Slice 2 deliverable (Issue #9197 S2)

The L1 call-site inline cache is now keyed by an **exact interned id sequence**
instead of an unverified `u64` hash (current fact 1 retired). A cache hit
requires exact `SmallVec<[ConcreteTypeId; 4]>` equality — the sjulia analogue of
upstream's `sig_match_fast` pointer check — so the L1 layer can no longer
conflate two distinct argument signatures (the `SubArray{Int64,1}` vs
`SubArray{Float64,2}` headline is now impossible at L1; pinned by the
`call_site_inline_cache_distinguishes_subarray_shapes_by_typeid_issue_9197`
regression test).

Wiring mechanism (per "Single-threaded VM policy"): the `TypeInternTable` is a
plain owned `Vm` field (`type_intern`), constructed by
`build_call_site_intern_tables()` in both `Vm::new` / `Vm::new_program`. That
builder also pre-interns the scalar value and scalar array-element ids into a
`CallSitePrimitiveTables` `Vm` field so the hot id-derivation path maps a
primitive (`Int64`, `Vector{Float64}`'s element, …) to its id by **array index**,
never re-building a string key or probing the intern `HashMap` — the reason the
exact-id key is not slower than the old hash fold (measured roughly neutral on
`vm_dynamic_dispatch_benchmark`, well inside the ±5% gate).

Changed surface: `call_site_arg_type_ids` / `call_site_arg_type_id` /
`struct_dispatch_type_id` / `intern_array_element_type` replace
`hash_call_site_fingerprint` / `hash_call_site_value_tag` /
`hash_struct_dispatch_identity` (`vm/mod.rs`); `CallSiteCache` stores two
`CallSiteArgIds` way keys (`vm/mod.rs`); `call_site_arg_fingerprint(s)` /
`lookup_call_site_inline_cache` / `store_call_site_inline_cache` take/return id
sequences (`vm/state.rs`); the six `CallDynamic*` call sites thread the owned key
by `as_deref()` borrow. `CallSiteCache` remains **runtime-only** (not serialized),
so there is no bincode / cache-version impact. Invalidation is unchanged (the
`dispatch_generation` counter bumped by `note_method_table_mutation`; the
backedge refinement is still S6).

### Deviations / refinements to the S1 contract

- **`ConcreteTypeKey::Struct.name` is `Rc<str>`, not `Box<str>`.** S1 anticipated
  this ("wiring may switch struct-name storage to the already-interned `Rc<str>`
  `struct_name` so interning a struct key is a refcount bump"); S2 realizes it so
  a general-struct dispatch clones the instance's `struct_name` by refcount bump
  with an empty (non-allocating) `params` vec — no per-call heap allocation.
- **Coarse composite keys where S1 left structured decomposition to S4.** A
  general struct interns `Struct { name: <fully-resolved dispatch name>, params:
  [] }` (the whole resolved name carries the parameters, matching the removed
  `hash_struct_dispatch_identity`); an array/memory element with a structured
  `ArrayElementType` (`StructOf`/`TupleOf`/`UnionOf`/`Abstract`/`StructInlineOf`)
  interns a `Debug`-derived `Primitive` name. Both are **injective over the
  dispatch identity** (sound for the exact-match key) but defer structured
  `params` access to S4.
- **`Range` element and step** are interned from the visible Julia type names
  carried by `RangeValue` (`UnitRange{T}` / `StepRange{T,S}`), rather than the
  `RangeElementType` discriminant. This keeps the call-site id inspectable and
  synchronized with `typeof` / `Value::runtime_type()` for native ranges while
  preserving an exact structural key (Issue #9815).
- **Value-kind coverage is unchanged**: exactly the kinds the removed
  `hash_call_site_value_tag` tagged produce an id; every other kind returns
  `None` and skips L1, identical to the previous policy.

## Slice 3 deliverable (Issue #9197 S3)

The **L2 dispatch cache** (`Vm::dispatch_cache`) now keys on the **same interned
`ConcreteTypeId` sequence** the L1 inline cache uses, instead of a runtime hash
of `get_type_name` strings (current fact 2 retired). The field type changed from
`HashMap<usize, HashMap<u64, usize>>` to `HashMap<usize, HashMap<CallSiteArgIds,
usize>>`; a hit is deterministic **exact id-sequence equality** (`HashMap` hashes
the id slice internally), not the pre-S3 unverified type-name hash — so the L2
layer, like L1 since S2, can no longer conflate two distinct argument signatures.

All four instruction handlers that share the L2 cache migrated together (they
share the one map): `CallDynamic`, `CallDynamicOrBuiltin`, `IterateDynamic`
(`vm/exec/call_dynamic.rs`), and `CallDynamicBinary`
(`vm/exec/call_dynamic_binary.rs`). Each **reuses the L1 `arg_fingerprint`** it
already computed — there is no second id-derivation path — so the L2-miss path no
longer builds any type-name string (`get_type_name` / the removed
`hash_type_names_iter` / `dynamic_dispatch_type_name` are gone from these sites;
`hash_type_name` stays for the still-string-keyed binary-both / method caches,
out of S3 scope). `lookup_call_site_dispatch_cache` / `store_call_site_dispatch_cache`
(`vm/state.rs`) take/insert `&[ConcreteTypeId]` / `CallSiteArgIds`. The cache
stays **runtime-only** (not serialized), so there is no bincode / cache-version
impact. `usize::MAX` remains the negative/builtin-fallback sentinel.

**Bounded overflow eviction.** `enforce_dispatch_cache_limit` (`vm/state.rs`) no
longer `.clear()`s the entire L2 cache on overflow (the pre-S3 cliff that dropped
every resolved-method decision at once). It now evicts **only the excess** —
exactly `entries - limit` entries, in `HashMap` iteration order (≈ random
replacement, O(1)-cheap) — so hot call sites keep their entries across an
overflow. This is **capacity management only**: the generation-counter
invalidation in `note_method_table_mutation` (which still whole-clears on a
method-table mutation) is **unchanged** — precise per-edge flushing remains S6.
The `#8625` clear counters still fire (one eviction event per overflow), so the
`runtime_cache_limits_bound_memory_stats_issue_8610` bound test is preserved.

### Deviations / refinements to the S1 contract

- **Untracked value kinds now skip L2 too (was: string-key cached).**
  **⚠️ RESOLVED by Issue #9427 — see "Untracked-kind re-caching" below.** The S1
  registry covered exactly the value kinds `hash_call_site_value_tag` tagged;
  kinds with no tracked dispatch identity (`DataType`/`Type{T}`, closures,
  generators, IO, data-typed arrays) yielded `arg_fingerprint == None`. Those
  already skipped L1; under S3 they skipped L2 as well and **re-resolved every
  call**. S3 assessed this as correctness-neutral (the resolver is authoritative)
  — which held — but **not** performance-neutral: bundled packages
  (Plots/Symbolics/SciML) dispatch closures and `Type{T}` arguments pervasively,
  so `packages::chunk_003` regressed 50 s → ~294 s (**6×**, Issue #9427). The
  correctness-only branch gates and the monomorphic-Int/Float dispatch bench both
  missed it. **#9427 re-caches these kinds via the `Opaque` interned key**
  (restoring the pre-#9404 string-key partition as ids), which is what S5's
  "re-cache untracked kinds" pulls forward. The pre-S3 `CallDynamic` special-case
  that rendered `DataType` as `Type{Name}` for the string key is now reproduced by
  the `Opaque("Type{<name>}")` id.
- **The candidate mismatch filter is left as-is (already off the hot path).**
  `dynamic_candidate_arg_mismatch` runs **only on the full-resolve path** (both
  caches missed), never on an L1/L2 hit, and its two Dict checks are
  carrier-removal `false`-returning stubs (`vm/util.rs`) while the range check
  inspects the `Value::Range` **structurally** (no `get_type_name`). Its
  `expected_type` is a *static compile-time candidate signature* string, not a
  runtime-derived type name, so there is no runtime string derivation to retire
  here; id-based candidate indexing is S5's `FirstArgIndex`→typemap scope. S3
  therefore only confirms the name usage is already off the hot path.
- **L2 key uses `CallSiteArgIds` (`SmallVec<[ConcreteTypeId; 4]>`), not a
  reduced hash.** `SmallVec: Borrow<[ConcreteTypeId]>` and its slice-consistent
  `Hash` let `lookup` probe by `&[ConcreteTypeId]` (no key allocation) while
  `store` interns the owned key with a stack-only `SmallVec::from_slice` for the
  common ≤4-argument case.

## Untracked-kind re-caching (Issue #9427 — S5 pulled forward)

**Problem.** S3 (PR #9404) re-keyed the L2 dispatch cache on interned
`ConcreteTypeId` sequences and made every argument kind with no id derivation
(`arg_fingerprint == None`) skip L2 as well as L1 — closures, `DataType` /
`Type{T}`, and every other previously-untracked kind. Before S3 the L2 cache
keyed those via runtime type-**name** strings (`get_type_name` /
`dynamic_dispatch_type_name`), so closure/Type-heavy call sites cached. Bundled
packages dispatch closures and type objects pervasively, so the skip turned into
a **full method-resolver run on every such call**: `packages::chunk_003` went
50 s → ~294 s (6×). It slipped through two "green" gates because the dispatch
bench used monomorphic Int/Float args (all tracked) and the branch full suite
compared correctness only, not per-chunk wall time.

**Fix.** Extend the id derivation (`call_site_arg_type_id`, `vm/mod.rs`) to
intern a real id for the previously-untracked kinds via the new
[`ConcreteTypeKey::Opaque(name)`](#the-intern-key-includes-full-type-parameters-by-child-id)
variant, whose `name` is **exactly** the pre-#9404 dispatch-name string. Because
the id partition equals the (correct, verified) pre-#9404 L2 string-key
partition, re-caching is sound by construction — distinct dispatch names ⇒
distinct ids, so the cache never conflates two dispatch-distinct values — and the
`Opaque` variant tag keeps these ids from colliding with same-spelling
`Struct`/`Enum`/`Primitive` ids.

**Kinds now covered and their identity derivations:**

| Value kind | Interned identity (`Opaque` unless noted) | Why sound |
|---|---|---|
| `DataType` (`Type{T}`) | `Type{<JuliaType::name()>}` (parameter-inclusive) | `Type{T}` dispatch discriminates on `T`; `name()` renders the full parametric spelling ⇒ `f(Int)` and `f(Vector{Float64})` keep distinct keys. This restores the `dynamic_dispatch_type_name` precision the main `CallDynamic` path had pre-#9404 (finer than the bare `get_type_name` "DataType" the two secondary paths used). |
| `Function` / `Closure` | `typeof(<name>)` | A callable singleton dispatches only by its singleton type; the captured environment is **not** part of the type (matching the retired `get_type_name`), so the definition-site `name` alone is the precise identity — two closures from one site dispatch identically, and conflating them is correct. |
| `ComposedFunction` | `ComposedFunction` | All composed functions share one dispatch type (matches `get_type_name`). |
| `Module` / `IO` / `Generator` | `Module` / `IOBuffer` / `Base.Generator` | Nominal singletons, one dispatch type each. |
| `Rng` | concrete `StableRNG` / `Xoshiro` / `MersenneTwister` / `TaskLocalRNG` | Concrete RNG type per handle (global handle = `TaskLocalRNG`, #7230). |
| `RuntimeTypeVar` / `RuntimeTypeName` / `SimpleVector` | `TypeVar` / `Core.TypeName` / `Core.SimpleVector` | Reflection singletons. |
| `Expr` / `QuoteNode` / `LineNumberNode` / `GlobalRef` | same-named singletons | Macro-AST kinds (hot in Symbolics / macro-heavy packages). |
| `SliceAll` | `Colon` | Indexing marker. |
| `StaticArray` / `StaticArrayInline` / `MemoryRef` | full parametric name via `julia_type_name[_owned]()` | Pure value method, exact parametric identity. |
| `Ref(T)` | **`Struct { name: "Base.RefValue", params: [id(T)] }`** (structural) | Recurses into the boxed element like `Tuple`; an untracked element ⇒ `None` (skip), matching the composite policy. |

**Kinds deliberately still `None` (skip L1/L2, re-resolve — correctness-neutral).**
Audited and justified as cold / dispatch-lossy, so caching them buys little and a
sound key is not cheap:

- `ExprArgs` — the legacy `Vector{Any}` `expr.args` carrier (Issue #6807);
  confined to `Expr` AST manipulation, its array-type identity is derived on the
  native-array path elsewhere, not a scored numeric hot path.
- `Pairs` — the kwargs carrier; its pre-#9404 name key was already
  first-value-lossy, and it is rarely a *positional* dispatch argument.
- `Undef` — `#undef` uninitialized-field sentinel; essentially never a dispatch
  argument.

**Soundness tests** (mirroring the S2 SubArray regression pattern): distinct
`Type{T}` params ⇒ distinct ids + no false L2 hit; function singleton by name;
closure identity = name not captures; distinct RNG kinds distinct; and the
cross-variant no-collision guarantee (`Type{Foo}` / struct `Foo` / function `Foo`
/ module `Foo` all distinct). See
`call_site_fingerprint_*_issue_9427` (`vm/tests.rs`) and
`opaque_key_renders_and_never_collides_across_variants_issue_9427`
(`subset_julia_vm_bytecode/src/type_intern.rs`).

**Prevention.** Dispatch-slice changes must now compare `packages::chunk_*` wall
time before/after (a 6× fixture-chunk slowdown passed two correctness-only
gates). Added to the dispatch-change checklist in `docs/vm/CHECKLISTS.md`
(referencing #9427).

## Slice 4 deliverable (Issue #9197 S4)

Slice 4 retires the runtime *type-name string parsing* named in fact 3
(`parametric_type_args` / `split_parametric_args`, formerly `vm/mod.rs`) and lands
the canonical structural extractor those consumers should use instead.

**Structural extractor (landed, S5-consumed).**
`TypeInternTable::intern_type_name(&mut self, name: &str) -> ConcreteTypeId`
(`subset_julia_vm_bytecode/src/type_intern.rs`) is the single place a *rendered concrete type
name* is decomposed into a structured [`ConcreteTypeKey`] DAG: the braces are
parsed **exactly once** at intern time and every type parameter is referenced by
its own interned `ConcreteTypeId`, so thereafter identity is by id — never by
re-splitting the name on each consultation (the fact-3 property). Recognized
parametric families map onto the structural variants (`Tuple`, `Array`/`Vector`/
`Matrix`, `Memory`, `UnitRange`/`StepRange`); any other braced base becomes a
nominal `Struct { name, params }` with structurally-interned parameters, so
`SubArray{Int64, 1}` and `SubArray{Float64, 2}` get **distinct** ids (the
conflation `type_id` cannot express, now via decomposed child ids rather than the
S2 coarse `Struct { name: <full string>, params: [] }`). It carries a narrow
`#[allow(dead_code)]`: the production consumer is S5's typemap first-arg indexing
(the dispatch RESOLVE path still matches on `JuliaType::Struct(name)` strings),
mirroring how S1 landed the registry unwired for S2 to consume. Equivalence is
pinned by `intern_type_name_top_level_params_match_legacy_split_issue_9197` (the
structured top-level params render **exactly** what a faithful copy of the retired
`split_parametric_args` produced, for `Dict{K,V}` / `SubArray` / nested
parametrics), `intern_type_name_round_trips_representative_shapes_issue_9197`
(`Array{T,N}` / `Vector` / `Matrix` / `Memory` / `UnitRange` / `StepRange` / nested
round-trip losslessly), and `intern_type_name_decomposes_params_structurally_issue_9197`
(param-distinctness + DAG child-id sharing — the behaviour-correcting improvement).

**Deleted string parsers.** `parametric_type_args`, `split_parametric_args`, and
`substitute_parent_type_arg` are removed from `vm/mod.rs`. Their only remaining
consumer was the **cold display path** — the show-method supertype walk
`projected_direct_parent_type_name` (← `user_show_method_for` ← print / string /
repr / REPL echo), *not* a dispatch/hot path (S2/S3 had already moved the cached
L1/L2 dispatch identity off strings). That walk now returns the parent type name
**as declared** in the struct hierarchy and no longer substitutes concrete
parameters: the substitution was functionally dead because every lookup keys on
the *family* name (`show_method_for_type_name` always falls back to the bare base,
and `pipeline_ctx::register_show_type_name` registers that bare base for *every*
parametric `show` signature), so the parent's concrete parameters never changed
which `show` was found. Verified behaviour-identical by the full fixture suite and
byte-identical decision classes under `SJULIA_DISPATCH_COMPARE=1`.

**Left for S5 (candidate filtering / typemap — tagged `(Issue #9197 S5)` in-code).**
These runtime type-name parameter parsers remain because they live on the dispatch
RESOLVE path (or a constructor/lowering path) and operate on `JuliaType::Struct(name)`
opaque spellings / serialized string templates, which only S5's typemap first-arg
indexing (id-based candidate matching) and a structured `JuliaType` parameter
representation can retire — forcing them now would be an ad-hoc rewrite, not the
structural upstream-compatible path:

- `vm/dispatch.rs`: `type_matches` (matches a `runtime_type: &str` candidate),
  `split_runtime_parametric_name`, `bind_ntuple_params`,
  `extract_type_bindings_for_selected_method` — dispatch RESOLVE / type-var
  binding, reached only on an L1/L2 cache miss.
- `vm/util.rs` `parse_parametric_params` (bytecode helper) — consumed by
  `vm/dispatch.rs`, `vm/dispatch_binding.rs`, `vm/builtins_types.rs`,
  `vm/builtins_arrays.rs` for runtime type matching / reflection.
- `vm/exec/call_dynamic.rs`: `top_level_generic_args` / `generator_iter_type_name`
  — `Base.Generator{…}` element-type extraction for iterator-size inference.
- `vm/exec/struct_ops.rs`: `parse_explicit_parametric_type_args` — constructor
  path (`Foo{Int64}(…)`); needs structured type-argument lowering, not the typemap.
- `subset_julia_vm_bytecode/src/array_element.rs`: `type_name_typeinfo_implicit` /
  `split_parametric_name` / `split_top_level_commas` — array-show `typeinfo`
  (display), string-template based on the serialized `Abstract(String)` tag.

The measurable slice goal — **zero runtime type-name string parsing on the cached
dispatch hot path** — was already met by S2/S3 (L1/L2 keyed on interned ids); S4
removes the last string parsers from the display path and provides the structural
extractor S5 wires into the resolver. No L1/L2 cache, `call_site_arg_type_id`
derivation, or cache-eligibility change ⇒ `packages::chunk_003` wall time and the
`vm_dynamic_dispatch_benchmark` are neutral (the #9427 gate).

## Slice 5 deliverable (Issue #9197 S5)

Slice 5 turns the `MethodTable` first-argument index (`FirstArgIndex`,
`runtime_types/method_table.rs`) from the #9112 **primitive-only** bucket into a
subtype-resolving **typemap**, mirroring the upstream `jl_typemap_level_t`
`arg1` / `tname` split (`julia/src/typemap.c`, `jl_typemap_level_assoc_exact`),
adapted to sjulia's single-threaded VM. It retires fact 4 ("`FirstArgIndex`
buckets primitives only … struct / abstract first-args all fall to `wildcard`
and are linearly scanned") for the primitive-argument dispatch path.

**Index structure (landed).** `FirstArgIndex` now buckets **every** method by
its declared first parameter's nominal family:

- `nominal: HashMap<String, Vec<usize>>` — a method whose first parameter is a
  sealed `Primitive`, a `Struct` family (`nominal_family_name`, e.g.
  `Complex{Int64}` → `"Complex"`), or a builtin `Abstract`
  (`CoreType::builtin_type_name`, e.g. `Number`) is keyed under that family.
- `wildcard: Vec<usize>` — a method whose first parameter matches structurally in
  ways a single nominal family key cannot express (`Any`, a type variable, a
  `Union` / `UnionAll`, a `Tuple` / `NamedTuple`, `Vararg`, `Type{T}`, a value
  parameter, `Bottom`, a `Module`, an `AbstractUser` user-declared abstract, or a
  `Named` opaque spelling) stays in the always-scanned bucket.

Buckets hold method indices in insertion order, so a gathered candidate list is
still definition-order deterministic (the #8641 ratchet).

**Subtype resolution (landed for the sealed-primitive argument).** For an actual
first argument that is a **sealed primitive**, the gather
(`primitive_candidate_indices`) walks the primitive's *complete* builtin
supertype chain — `Int64 <: Signed <: Integer <: Real <: Number`,
`Float64 <: AbstractFloat <: Real <: Number`, `String <: AbstractString`, … —
via [`CoreType::direct_builtin_supertype_name`] /
[`CoreType::direct_builtin_supertype_name_for_julia_name`] (**the shared
`CoreType` hierarchy, not a second string-parse derivation**) and gathers the
primitive's own bucket, every abstract-supertype bucket on that chain, and
`wildcard`. This is subtype-resolved indexing: an `Int64` dispatch finds a method
registered on its abstract supertype `Number` / `Real` while **skipping** every
`Struct`/container/unrelated-abstract bucket (`Complex`, `Rational`, `Float64`,
`AbstractFloat`, `AbstractArray`, …) a primitive is a subtype of none of.

**Why this is sound.** A sealed primitive's supertype set is a fixed, complete
chain, so the gather is a **superset** of every method whose first parameter is a
supertype of the argument — no matching method is ever dropped. Bucketing is only
a candidate-*set* restriction; the scorer still rejects the non-matches, so the
selected method is byte-identical to the prior full/`wildcard` scan. A chain that
ever fails to reach `Any` returns `None` (defensive full-scan fallback; never
fires for the current sealed primitives). Pinned by
`first_arg_index_buckets_by_nominal_family_issue_9197`,
`first_arg_gather_is_complete_and_sound_for_primitive_issue_9197`, and
`first_arg_gather_preserves_specificity_ordering_issue_9197`
(`runtime_types/method_table.rs`).

**Candidate-scan reduction (acceptance, #9112-class).** On a representative
dispatch-heavy table (12 methods: primitive same-type, numeric abstracts, several
`Struct` methods, one `Any` catch-all), an `Int64` dispatch now scans **4**
candidates instead of the 12 the full/`wildcard` scan visited — pinned
deterministically by `first_arg_index_reduces_candidate_scan_count_issue_9197`
via the `#[cfg(test)]` `dispatch_candidate_scan_count()` hook (a single
per-dispatch `Cell` add, compiled out of production builds). The reduction lands
on the hottest dispatch path: before S5 the struct-first methods (`Complex`,
`Rational`, …) sat in `wildcard` and were scanned on **every** `Int64 + Int64` /
`Float64 * Float64`; they are now skipped.

**Deferred to S6/S7 (tagged in-code).** The **struct/abstract-argument** gather is
NOT enabled this slice: a struct argument (e.g. `Vector{Float64}`) still takes the
unchanged full scan, because a complete abstract-container supertype enumeration
(`Vector <: AbstractVector`, a skip-level the nominal builtin walk
`struct_direct_supertype_name` does not model) needs the structured id relation —
the same reason the `vm/dispatch.rs` runtime type-name parsers (`type_matches`,
`split_runtime_parametric_name`, `bind_ntuple_params`,
`extract_type_bindings_for_selected_method`) and `vm/util.rs`
`parse_parametric_params` on that struct-argument resolve path are left for
S6/S7. Their existing `(Issue #9197 S5)` markers are re-scoped to S6/S7 (they were
never on the primitive hot path S5 delivers).

**Invalidation / caching unchanged.** `FirstArgIndex` is a `#[serde(skip)]`
runtime field rebuilt lazily and cleared to `None` by every `add_method` /
`add_method_keep_existing` (the #9112 generation model); `note_method_table_mutation`
whole-clear invalidation is untouched (backedge refinement remains S6). The index
is not serialized, so there is no bincode / cache-version impact (the cache
boundary is S7). `vm_dynamic_dispatch_benchmark` and `packages::chunk_003` wall
time are neutral-to-improved (fewer candidates matched per cache-cold dispatch).

## Slice 6 deliverable (Issue #9197 S6)

Slice 6 replaces the coarse whole-clear that every eval-time method
(re)definition did on the runtime dispatch caches (fact-6-adjacent: the
`note_method_table_mutation` "wipe every decision map + bump the global
generation" v1 noted in the "Invalidation story" above) with **per-name precise
invalidation**, so redefining one method no longer evicts warm call sites of
unrelated generic functions.

**Upstream shape.** Upstream Julia's `jl_method_table_insert` bumps the world
counter and walks backedges to cap only the `CodeInstance`s whose dispatch could
change (`invalidate_backedges`, `julia/src/gf.c`) rather than flushing the whole
method cache. S6 is the runtime-dispatch-cache analogue, adapted to sjulia's
single-threaded flat function table.

**Why not the #8553/#8554 graph directly.** The precise backedge graph landed by
#8553/#8554 (`compile/abstract_interp/engine/backedges.rs`, `BackedgeIndex` keyed
by `SpecializationKey`/`WorldRange`) is **inference-time only**: it invalidates
the *IPO/type-inference* cache, and its keys are canonical specialization
signatures, not the runtime VM's `(call_site_ip, interned arg-id sequence) →
func_index` dispatch caches (S2/S3). It is not held by the runtime `Vm` and does
not model runtime cache entries, so wiring it to the runtime caches would be a
key/lifecycle mismatch, not a reuse. S6 instead builds the **minimal runtime edge
set**: a runtime dispatch cache entry's implicit backedge is its resolved
function index, so the reverse "callee name → dependent entries" map is
recomputed on demand from the cached `func_index` — no persisted reverse index
(single-threaded VM; the on-demand scan is cheap because the HashMaps hold only
live entries and eval-time mutations are rare).

**Mechanism (`vm/state.rs`, `vm/mod.rs`).** `activate_eval_function` — the sole
production method-table mutation path — knows the mutated generic function name
and calls the new `note_method_table_mutation_for(name)`:

- `dispatch_decision_affected(functions, target, func_index)` decides per entry:
  drop iff `func_index == usize::MAX` (builtin/native fallback sentinel — a fresh
  user method for `target` may now capture the site), the index is out of range
  (defensive), or the resolved method's own generic-function base name equals
  `target`. Base names compare after the last `.` (alias-tolerant, like the
  #8554 `function_base_name` walk).
- L2 `dispatch_cache`, `binary_both_dispatch_cache`, and `method_dispatch_cache`
  `retain` only unaffected entries (a `None` negative decision, whose name the
  hashed key cannot recover, is dropped conservatively).
- L1 `call_site_caches` vacates only the affected ways per slot
  (`CallSiteCache::invalidate_ways`, which LRU-compacts a surviving way up to
  MRU) and — crucially — does **not** bump `dispatch_generation`, so unrelated
  inline-cache slots stay live (the whole point of "fewer evictions").
- The negative `specialization_failure_cache` is still cleared wholesale
  (correctness-neutral to re-run; a new definition can turn a failure into a
  success, #8603).

The coarse `note_method_table_mutation` whole-clear (generation bump + full map
clears) is kept as the fallback for a nameless caller and for
`clear_runtime_caches` (host-facing reset, #8453).

**Correctness (paramount).** The set of entries dropped for `name` is exactly the
set the whole-clear would drop-and-recompute for `name`, so any re-resolution of
a call site dispatching `name` is byte-identical to the pre-S6 behaviour; kept
entries belong to unrelated generic functions whose dispatch is independent of
this mutation (upstream: a method of `f` is selected only from `f`'s method
table). This is the standard bucketed-by-name soundness — a **superset** of the
finer signature-intersection set every entry the #8554 overlap test would
invalidate has the same generic-function name. Pinned by
`note_method_table_mutation_for_evicts_the_redefined_method_issue_9197_s6`
(a redefined method's cached call site misses so the next call re-resolves — the
#8452/#9400 world-age family), the end-to-end
`dispatch/eval_method_redefinition_after_warmup_8561` and
`dispatch/precise_cache_invalidation_unrelated_method_9197` fixtures (julia
parity), and the full `--release` fixture suite. The change touches only the
invalidation side (never the dispatch RESOLVE path or the typemap), so dispatch
decision classes are unchanged.

**Precision (acceptance).** `note_method_table_mutation_for_preserves_unrelated_*`
(L1 inline cache and L2 dispatch cache) assert that redefining `f` keeps `g`'s
warm entries hitting, and `note_method_table_mutation_for_vacates_only_the_affected_l1_way`
asserts per-way precision (one polymorphic-callable slot's `f` way is vacated
while its `g` way survives and is promoted). `note_method_table_mutation_for_drops_builtin_fallback_entries`
pins the `usize::MAX` soundness case.

**No bincode / cache-version impact.** No new serialized state: the invalidation
reads existing `FunctionInfo.name`; the L1/L2 caches remain runtime-only. The
cache boundary is S7.

**Deferred to S7.** Finer *signature-intersection* invalidation (drop only the
entries whose argument ids intersect the mutated method's signature, using the
#8554 `spec_tuples_may_overlap` machinery) instead of dropping the whole
generic-function bucket; and — if eval-time mutations ever become a hot path — a
materialized `name → call-site IPs` reverse index to avoid the O(code.len()) L1
slot scan.

## Slice 7 deliverable (Issue #9197 S7)

Slice 7 retires fact 6's **cache-boundary** string key: the serialized Base
cache's `SerializedBaseCache.method_tables` map was keyed by a bare `String`
(`compile/precompile.rs`), the last string-keyed dispatch structure at the cache
boundary. It is now keyed by the typed [`MethodTableKey`]
(`runtime_types/method_table.rs`), and the section serializes deterministically.

### The key is a *generic-function* identity, not a `ConcreteTypeId`

An important correction to the S1 contract's framing (which grouped this under
"typed (`ConcreteTypeId`) keys across the Base-cache and REPL boundaries"): the
`method_tables` key is a **generic-function name** (`"+"`, `"Base.log2"`, a
module-qualified `"Module.f"`, a constructor's bare type name `"Foo"`, a nested
`"parent#nested"`), **not** a concrete-type name. A generic function is not a
concrete type, so folding this key into the `ConcreteTypeId` intern registry
would be a semantic mismatch (Design Principle #2/#8 — no ad-hoc structural
misuse). `MethodTableKey` is therefore a dedicated *generic-function* identity —
the sjulia analogue of upstream Julia owning a method table by an interned
generic-function/type-name identity (`jl_typename_t` / `typeof(f)`,
`julia/src/staticdata.c`) rather than a re-parsed name — kept distinct from the
type-identity `ConcreteTypeId`.

### What landed

- **Typed key.** `MethodTableKey(String)` — `Ord`/`Hash`/`Eq`/`Serialize`/
  `Deserialize`, serializing transparently as its canonical name (a stable,
  deterministic string, never a per-process id — the #9473-class requirement).
  Because bincode's `serialize_newtype_struct` forwards to the inner `String`,
  the wire is byte-compatible with the pre-S7 bare `String` key on the
  deserialize side; the only header change is `version: u32` 87 → 88.
  `SerializedBaseCache.method_tables` is now
  `HashMap<MethodTableKey, MethodTable>`; the in-memory dispatch/inference tables
  stay `String`-keyed and unwrap at the cache→compiler boundary
  (`cached_base_from_serialized`, `compile/cache.rs`) — the full in-memory
  conversion is #9199-gated (see below).
- **Deterministic serialization (fixes a latent #9473-class bug).**
  `serialize_base_cache` emits both the `method_tables` **and** `closure_captures`
  sections **sorted by key** (inner capture-name sets sorted too). Before S7 the
  section-based `append_section` serialized the raw `HashMap` via
  `cache_codec().serialize(...)`, iterating in **per-process hash-seed order** —
  the struct's `#[serde(serialize_with = "sorted_hashmap")]` /
  `sorted_hashmap_of_hashset` attributes only fire on the (test-only) whole-struct
  serialize path, so the active section path bypassed them. The whole Base cache
  (`--precompile-base`) is now **byte-identical across independent processes**;
  before S7 two runs differed in the `method_tables` + `closure_captures` regions.

### CACHE_VERSION / fingerprint

`CACHE_VERSION` 87 → 88 (this PR is the wave's designated cache-bumper). The
schema-source fingerprint is regenerated
(`src/compile/base_cache_schema_fingerprint.txt`,
`scripts/audit_base_cache_schema_fingerprint.sh` green) — this also corrects a
**pre-existing stale snapshot** (it read `CACHE_VERSION=86` while the code was
already at 87 after #9198 S4 bumped it without refreshing the fingerprint). Old
persistent/embedded caches miss on both the version gate and the changed
schema/build fingerprint and are regenerated cleanly.

### Verification

Cold clear-and-regenerate (v88 rebuilds Base from source, correct output) +
warm load (0.06 s); cross-process `--precompile-base` byte-identical;
`SJULIA_DISPATCH_COMPARE=1` over dispatch-heavy fixtures → **zero
`selection-diff`** (the change is orthogonal to every dispatch/scoring/typemap
path, so decision classes are unchanged vs origin/main); round-trip +
determinism + typed-key ordering unit tests
(`method_tables_serialize_deterministically_with_typed_key_issue_9197_s7`,
`method_table_key_wraps_name_orders_and_serializes_transparently_issue_9197_s7`).

### What #9197 still leaves open (why S7 is "Part of", not "Closes")

The epic's five acceptance criteria are substantially met by S1–S6 (exact-match
L1/L2 keys, `parametric_type_args` retired from the hot path, primitive first-arg
typemap, per-name precise invalidation), and S7 retires the last **cache-boundary**
string key with deterministic serialization. Remaining, tracked work:

- The **in-memory** `HashMap<String, MethodTable>` dispatch/inference tables stay
  `String`-keyed; converting them to `MethodTableKey` (or persisting the intern
  registry) is **#9199-gated** — #9199 must persist session state across REPL
  evals before the `struct_name → type_id` re-resolution (`repl/session.rs`) can
  be retired (design-record S7's original scope).
- The **struct/abstract-argument** `FirstArgIndex` gather and the `vm/dispatch.rs`
  runtime type-name resolve-path parsers remain (deferred by S5), needing the
  structured abstract-container supertype relation.
- Finer **signature-intersection** cache invalidation (deferred by S6).

`MethodTableKey` is the boundary type these follow-ups build on.
