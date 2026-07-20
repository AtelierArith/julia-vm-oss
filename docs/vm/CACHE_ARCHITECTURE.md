# Cache Architecture in SubsetJuliaVM

This document describes the thread-local state management pattern used during
Base compilation, and the invariants that must be maintained between the Base
cache and associated registries.

## Overview

The representation and validity decision for the cache stack is recorded in
[`CACHE_IR_RFC.md`](CACHE_IR_RFC.md). Issue #10051 closed the structural audit
with a hybrid IR/bytecode design rather than a single cache format.

The accepted design for reconstructing compile-time semantic state after every
cache hit is [COMPILE_CONTEXT_REHYDRATION.md](./COMPILE_CONTEXT_REHYDRATION.md)
(Issue #10438). Production migration and the mismatch scoreboard are tracked by
Issue #10462.

SubsetJuliaVM uses thread-local caches and registries to avoid recompiling
the Julia Base library on every `compile_with_cache()` call.

There are two kinds of thread-local state populated during Base compilation:

| Name | Kind | Location | Cleared by |
|------|------|----------|------------|
| `BASE_CACHE` | Cache | `cache.rs` | `clear_cache()` |
| `PROGRAM_CACHE` | Cache | `cache.rs` | `clear_cache()` |
| `PROGRAM_CACHE_SEEN` | Registry | `cache.rs` | `clear_cache()` |
| `PROMOTION_RULE_REGISTRY` | Registry | `promotion.rs` | `clear_cache()`, `promotion::clear_registry()` |
| `show_methods` | Field in `CompiledProgram` | `cache.rs` (embedded in `CachedBase`) | Implicitly cleared via `BASE_CACHE` |
| `inference_results` | Field in `CachedBase` / `SerializedBaseCache` | `cache.rs`, `precompile.rs` | Implicitly cleared via `BASE_CACHE` |

### PROGRAM_CACHE store policy (Issue #6348)

`PROGRAM_CACHE` stores a deep clone of the final `CompiledProgram` (~6 ms for a
Base-merged program). One-shot CLI runs never reuse it, so a program is stored
only on its SECOND compile of the same hash: the first compile just records the
hash in `PROGRAM_CACHE_SEEN`, and repeated compilations get full hits from the
third compile onward.

### Warm-start prefetch (Issue #6348, phase 2)

The Base cache contains VM `Value` constants (`Rc`-based, not `Send`), so its
deserialize must stay on the compiling thread. To overlap the two largest
warm-start deserializes anyway, the CLI:

1. calls `compile::cache::begin_warm_start_prefetch()` at process start — a
   background thread warms the prelude `Program` Lazy (`Program` is `Send`)
   and pre-clones `prelude.functions` for the shared inference engine;
2. calls `compile::cache::warm_base_cache()` on the main thread before
   `parse_and_lower`, so the ~9 ms Base-cache read + deserialize runs while
   the background thread loads the prelude.

`take_prefetched_base_inference_functions(expected_len)` hands the clone to at
most one compile and rejects length mismatches (Base-redefinition merges);
every consumer falls back to the regular clone path when the prefetch is
absent. On wasm both entry points are no-ops.

## Thread-Local Registry Invariant

> After any call to `compile_with_cache()`, all associated registries must be
> populated. This must hold even on cache hits (second call without clearing
> `BASE_CACHE`).

Violation of this invariant is what caused bugs #3036 and #2489:

- **#3036**: `PROMOTION_RULE_REGISTRY` was not stored in `CachedBase`. On a
  second compile where only the registry was cleared (but not `BASE_CACHE`),
  `compile_base_functions()` was skipped, leaving the registry empty.
- **#2489**: `show_methods` was not stored in `CachedBase`. Same structural
  pattern, same fix.

## Lifecycle

```
First call to compile_with_cache():
  └─► BASE_CACHE miss → compile_base_functions()
        ├─► Populate PROMOTION_RULE_REGISTRY
        ├─► mark_registry_initialized()
        ├─► Snapshot InferenceEngine return_type_cache
        └─► Store promotion_rules + inference_results in CachedBase → stored in BASE_CACHE

Second call to compile_with_cache() (BASE_CACHE still populated):
  └─► BASE_CACHE hit → get_or_init_base_cache()
        ├─► if !is_registry_initialized():
        │     Replay CachedBase.promotion_rules → PROMOTION_RULE_REGISTRY
        │     mark_registry_initialized()
        └─► Seed shared InferenceEngine from CachedBase.inference_results
```

## Adding a New Thread-Local Registry

If you add a new thread-local registry populated during Base compilation,
follow this checklist:

- [ ] Add a `clear_<registry>()` function in the registry module
- [ ] Call `clear_<registry>()` inside `clear_cache()` in `cache.rs`
- [ ] Add a field to `CachedBase` to store the registry contents
- [ ] Populate the field in `compile_base_functions()` after the registry is filled
- [ ] Replay the field in `get_or_init_base_cache()` when the registry is empty
  (check `is_<registry>_initialized()` before replaying)
- [ ] Write a regression test: `test_<registry>_populated_on_second_compile_with_cache`

## Invariant After `clear_cache()`

After `clear_cache()`, the following must all be true:

- `is_cache_initialized() == false`
- `promotion::is_registry_initialized() == false`
- `promotion::get_registry_size() == 0`

This is enforced by `clear_cache()` calling `promotion::clear_registry()` in
addition to clearing `BASE_CACHE` (Issue #3038).

## Partial Clear for the Fixture Harness: `clear_non_base_cache()` (Issue #9843)

Each `fixture_tests` chunk binary runs ~32 fixtures in one process. Before this
function existed, `run_test_case()` called `clear_cache()` before every
fixture, forcing `compile_base_functions()` to recompile the whole Base
library from scratch up to 32 times per chunk — expensive, and (per the
Thread-Local Registry Invariant above) it also meant Base was recompiled with
a fresh `HashSet`-ordered `capture_names` list each time, widening the window
for the #9769-class ordering flake.

`clear_non_base_cache()` clears `PROGRAM_CACHE`, `PROGRAM_CACHE_SEEN`, and the
promotion registry, but deliberately leaves `BASE_CACHE` populated. This is
safe under the same Lifecycle guarantee documented above: the next
`compile_with_cache()` call hits `get_or_init_base_cache()`, sees
`!is_registry_initialized()`, and replays `CachedBase.promotion_rules` instead
of recompiling Base — so the fixture harness gets per-fixture isolation for
program-level state while paying the Base compilation cost once per chunk
process instead of once per fixture.

## Fresh-Base Testing and Bisecting (Issue #5413)

The persistent Base cache can mask dispatch and inference bugs that only occur
when Base is compiled from source. The cache filename is keyed by Base source
content, so unrelated commits in a `git bisect` can reuse a cache produced by a
different binary. In that case the observed pass/fail result reflects the cache
producer, not necessarily the checked-out commit.

Use the no-cache path when investigating compile-time dispatch, inference, or
method-table behavior:

```bash
rm -f target/sjulia_base_cache_*.bin target/base_cache.bin
SUBSET_JULIA_VM_DISABLE_CACHE=1 \
SUBSET_JULIA_VM_DISABLE_PERSISTENT_BASE_CACHE=1 \
timeout 1800 cargo nextest run --release --test fixture_tests dispatch:: where_tests:: type_inference:: --no-fail-fast
SUBSET_JULIA_VM_DISABLE_CACHE=1 \
SUBSET_JULIA_VM_DISABLE_PERSISTENT_BASE_CACHE=1 \
timeout 1800 cargo nextest run --release --test type_propagation_call_tests --no-fail-fast
```

The CI `no-cache-dispatch-inference` job runs the same fresh-Base guard for the
dispatch, `where`, and type inference fixture categories plus the
`type_propagation_call_tests` integration suite.

## Cache Fingerprints and Status (Issue #8718)

Persistent and embedded Base caches carry an envelope header with the Base
source/schema hash, the compiler build fingerprint from
`SJULIA_BASE_CACHE_BUILD_HASH`, and the wire-format enum variant fingerprint.
The persistent Base cache path is also keyed by the combined cache hash, so a
new sjulia binary naturally misses an older cache path. If a stale file is
loaded directly, the header gate rejects it before payload deserialization and
the normal load path removes the stale file so Base is regenerated from source.

The parsed/lowered prelude Program cache is invalidated by the same compiler
build fingerprint and enum variant fingerprint. It uses a separate source hash
because it stores lowered IR rather than VM bytecode.

Use `sjulia --cache-status` to inspect cache selection without compiling,
reparsing, or deleting stale files. The command prints JSON with
`load_source` (`embedded`, `persistent`, or `none`), embedded/persistent
artifact states, paths, and the active fingerprints for both the Base bytecode
cache and the prelude Program cache.

## Prelude Program Cache (Issue #6026)

The parsed/lowered prelude Program is cached separately from compiled Base
bytecode. Native builds first try the process-local `PRELUDE_PROGRAM`, then use
the persistent `target/sjulia_prelude_program_<prelude-hash>.bin` file unless
`SUBSET_JULIA_VM_DISABLE_PERSISTENT_PRELUDE_CACHE=1` is set.

WASM builds cannot rely on the persistent filesystem cache, so release artifacts
can embed a prelude Program cache with:

```bash
SJULIA_PRELUDE_PROGRAM_CACHE=/abs/path/to/prelude_program_cache.bin
```

When present, `parse_and_lower()` deserializes that embedded Program instead of
parsing/lowering the full prelude source during the first `run_from_source()`.
The cache is validated with a format version and SHA-256 hash of
`base::get_prelude()`. `scripts/wasm_build_with_cache.sh` generates and embeds
this cache alongside `SJULIA_BASE_CACHE`.

## show_methods Cache (Issue #2489)

`show_methods` is a `HashMap<String, usize>` stored as a field in `CachedBase`
(not a separate thread-local registry). It maps function names to their
`global_index` for custom `show` methods registered during Base compilation.

The same "must be pre-populated on cache hit" invariant applies:

- **On cache miss**: `compile_base_functions()` detects show methods
  (condition: `is_base_extension || is_base_function`, name == `"show"`,
  params[0] == IO, params[1] == Struct) and populates `show_methods`.
- **On cache hit**: `show_methods` is restored from `CachedBase.show_methods`.
  The function loop skips cached Base functions, so detection would fail
  without pre-population.

**Important**: Do NOT use `global_index` from the detection loop for cached
entries — `global_index` starts at `cached_base_len` and is not incremented
for cached Base functions.

## Inference Results Cache (Issue #5093)

`inference_results` stores live `InferenceEngine::return_type_cache` entries
after Base source compilation. `compile_core_program_internal()` returns the
snapshot to `compile_base_functions_from_source()`, `CachedBase` keeps it for
same-process Base cache hits, and `SerializedBaseCache` persists it for
persistent/embedded Base cache loads.

On a Base cache hit, `CompilerCacheInput::inference_results` seeds the shared
inference engine before user functions are inferred. Persisted entries are
rebased from the producing process' world range to the loading engine's current
world; capped entries are skipped so invalidated results are not revived. Later
user or stdlib methods are registered through `InferenceEngine::add_method`, so
seeded Base entries that depend on a mutated method are invalidated through the
normal world-range/backedge path.

`CompiledProgram.specializable_functions` and the finalized
`CompiledProgram.specialization_disable_flags` are also serialized as part of
`SerializedBaseCache.compiled`, so Base runtime specialization targets and the
fresh compiler's fast-path safety decisions survive persistent/embedded cache
roundtrips. `CompiledProgram.compile_context` remains `#[serde(skip)]` and is
rebuilt by `cached_base_from_serialized()` from persisted semantic snapshots and
structural projections, so the serialized payload does not need to include full
prelude IR context.

## SerializedBaseCache (Issue #3240)

When modifying `SerializedBaseCache` fields or `CACHE_VERSION`:

- [ ] Increment `CACHE_VERSION`
- [ ] Verify `test_serialize_deserialize_roundtrip_empty_program` passes
- [ ] Verify `test_version_mismatch_returns_error` passes
- [ ] If the change touched a file listed in
      `src/compile/base_cache_schema_files.txt`, run
      `bash scripts/audit_base_cache_schema_fingerprint.sh --update` to
      refresh the checked-in snapshot in the same commit (see "Cache envelope
      validation gates" below and `docs/vm/CODE_AUDITS.md`).

### Cache envelope validation gates (Issues #5968 / #8444 / #8626 / #8627)

`deserialize_base_cache` validates, in order, before any payload decode:

1. `version` — streaming prefix read (`CacheVersionHeader`), so an older
   snapshot is rejected before the positional decode can misalign (#5968).
2. `magic` — `SJBCACH1` envelope marker.
3. `schema_fingerprint` — build-script hash of the wire-format source files
   listed in `src/compile/base_cache_schema_files.txt` (#8444), including
   `src/compile/instr_wire_ids.rs` (#8627). Guarded by
   `scripts/audit_base_cache_schema_fingerprint.sh` +
   `src/compile/base_cache_schema_fingerprint.txt`.

   `deserialize_base_cache` recomputes this hash from the *current* source
   tree every time it runs, so a cache built by any older binary is rejected
   at load time regardless of whether `CACHE_VERSION` or the checked-in
   snapshot were updated — the runtime path is self-protecting and does not
   depend on a human remembering anything (Issue #10051 slice A).
   `scripts/audit_base_cache_schema_fingerprint.sh` is a separate, **CI/review
   hygiene gate**: it fails a PR whose diff touches a schema-manifest file
   without also bumping `CACHE_VERSION` and refreshing the snapshot, so the
   change is visible in review instead of only surfacing as a cold-start
   cache miss. It generates its fingerprint from those same source files,
   hashed in `LC_ALL=C`-sorted path order (Issue #10051 slice A) so the
   audited hash no longer depends on the manifest's line order. For the
   manifest as of #10051 slice A this order happens to also match
   `build.rs::base_cache_schema_fingerprint()`'s `Vec<PathBuf>::sort()`
   (verified empirically), but the two are **not proven equal in general** —
   Rust's component-wise `Path` `Ord` and plain `LC_ALL=C` byte order diverge
   for paths that collide at a `.`-vs-`/` boundary (e.g. a hypothetical
   `value.rs` next to a `value/` directory) — so this audit checks the
   snapshot for internal consistency (bash "current" vs the committed
   "snapshot", the same algorithm both times), not literal equality with the
   value `deserialize_base_cache` computes above. Run
   `bash scripts/audit_base_cache_schema_fingerprint.sh --update` to refresh
   the snapshot after a deliberate schema change (see
   `docs/vm/CODE_AUDITS.md`).
4. `compiler_build_fingerprint` — build-script hash of all Rust sources in
   `subset_julia_vm/src` **and** in the sibling crates whose serde-derived
   types appear in serialized payloads: `subset_julia_vm_ir`,
   `subset_julia_vm_types`, `subset_julia_vm_bytecode`
   (#7515/#8444/#10332): a cache built by any other compiler build misses.
   The dependency-crate coverage exists because `Program`-side types
   (`Expr`, `JuliaType`, `TypeExpr`, `TypeParam`, `Span`) are positional
   bincode/postcard payload inputs that are neither in the schema manifest
   nor tracked by the enum-variant fingerprint; before #10332 a serde-shape
   change there invalidated nothing automatically and relied on a manual
   `CACHE_VERSION` bump (e.g. the `Expr::Convert` bump to 93). The hashed
   root list is exported as `SJULIA_CACHE_BUILD_FINGERPRINT_ROOTS` and
   pinned by the unit test
   `compiler_build_fingerprint_covers_payload_dependency_crates_10332`.
5. `enum_variant_fingerprint` — runtime hash (via `strum::VariantNames`) of
   the variant-name lists, in declaration order, of the wire-format enums
   `Instr` / `BuiltinId` / `Intrinsic` / `BuiltinOp` (#8626). For `BuiltinId`,
   `Intrinsic`, and `BuiltinOp` the declaration order still equals the wire ID
   after #8627 (wire IDs were assigned = current declaration indices for
   byte-compatibility), so reordering those enums after #8627 will correctly
   invalidate caches even before #8628's audit script enforces the constraint.

Every gate failure surfaces as a clean `Err`, and both loaders degrade
gracefully: `read_persistent_base_cache` deletes the stale file and returns
`None` (the caller recompiles Base from source and rewrites the cache), and
`load_embedded_cache` logs a warning and falls back to runtime compilation.
Embedded caches (iOS `build.sh`, `SJULIA_BASE_CACHE`) are generated by a
binary built from the same source tree, so their fingerprints match by
construction. The prelude Program cache (`pipeline.rs`) carries the same
`enum_variant_fingerprint` field (its lowered IR serializes `BuiltinOp`) with
the same discard-and-regenerate fallback.

### Determinism of Base cache serialization (Issue #10051 slice B)

`precompile_base_is_deterministic_across_processes`
(`subset_julia_vm/tests/sjulia_precompile_determinism_tests.rs`) requires two
independent `sjulia --precompile-base` processes to emit byte-identical
`base_cache.bin` for the same prelude. It regressed once (#9473, fixed by
#9532/#9197 S7) because `HashMap`/`HashSet` iteration order depends on the
per-process random hash seed. The subprocesses explicitly disable persistent
prelude and Base caches: otherwise both can deserialize the same local artifact
instead of exercising independent compiler runs, masking non-deterministic
bytecode emission such as closure capture layout (#11264). Tech-debt epic
#10051 solution B asked for a
fresh audit of every hash-based collection reachable from
`SerializedBaseCache` at serialize time; the inventory below is that audit
(`path:line` as of this slice):

| Source | Status | Where it is handled |
|---|---|---|
| `method_tables: HashMap<MethodTableKey, MethodTable>` | Sorted by typed key before the section is written | `precompile.rs:531-536` (`serialize_base_cache`) |
| `closure_captures: HashMap<String, HashSet<String>>` | Sorted by outer key and inner set during metadata serialization; every `CreateClosure.capture_names` emission also sorts before the order becomes bytecode layout | `precompile.rs:545-554`, `compile/{expr/mod.rs,expr/call/mod.rs,stmt.rs}` |
| `promotion_rules: Vec<(String,String,String)>` extracted from a registry `HashMap` | `.sort()` right after extraction | `precompile.rs:505-508` |
| `runtime_specialization_map: Vec<(usize,usize)>` built from `shared_ctx.spec_func_mapping: HashMap` | `.sort_unstable_by_key(...)` right after collection | `pipeline_ctx.rs:5244-5250` |
| `inference_results` | Always serialized as an empty `Vec` for persistent/embedded caches (never a live `HashMap` on this path) | `precompile.rs:561-565` |
| `MethodTable::dispatch_cache: RefCell<HashMap<CoreType, usize>>`, `first_arg_index`, `projection` | `#[serde(skip)]` — never enters the wire format | `subset_julia_vm_bytecode/src/method_table.rs:1047-1075` |
| `CompiledProgram::macro_bindings: HashMap<String, HashSet<String>>` | Not part of the Base-cache section list at all (`append_compiled_program_section` never writes it); reconstructed as `HashMap::new()` on load | `precompile.rs:782-841`, `:967` |
| `CompiledProgram::compile_context`, `main_scope_names` | `#[serde(skip)]` — runtime-only, rebuilt per compile | `subset_julia_vm_bytecode/src/program.rs:297`, `:330` |
| `CoreType`/`JuliaType`/`MethodSig`/`FunctionInfo`/`StructDefInfo`/etc. (types nested inside the sections above) | Structurally `Vec`/`Option`/scalar only — no embedded `HashMap`/`HashSet` fields found in this audit | `subset_julia_vm_types/src/inference_core/type_core/repr.rs`, `subset_julia_vm_bytecode/src/{program,metadata,method_table}.rs` |
| Core IR (`ir::core::Function`/`Expr`) inside `specializable_functions: Vec<SpecializableFunction>` (`ir: Arc<Function>`), and every `Instr` operand in `compiled.code` | Not individually hand-walked field-by-field (it is the largest and most structurally varied payload in the cache). Coverage here is **grep-negative** for the `Expr`/`Function`/`Instr`-operand types specifically — `ir/core.rs` has zero `HashMap`/`HashSet` occurrences; the two hits under `subset_julia_vm_bytecode/src/value/` (`enum_registry.rs`, `macro_.rs`) are process-local `thread_local! { RefCell<HashMap/HashSet> }` runtime registries, not `#[derive(Serialize)]` struct fields, so they never enter the wire format — **plus empirical**: `precompile_base_is_deterministic_across_processes` byte-compares the fully-encoded `compiled.code`/`compiled.functions` sections (which embed this IR) across independent processes, 5/5 green. This row's guarantee is weaker in kind than the sorted/`#[serde(skip)]` rows above (grep + empirical, not a field-by-field derivation), but is exercised on every run of that test | `subset_julia_vm_types/src/ir/core.rs`, `subset_julia_vm_bytecode/src/{value/*,program.rs}` |

No source of non-determinism was found beyond what #9473/#9197-S7 already
fixed, so this slice did not change any serialized byte layout (no
`CACHE_VERSION` bump). What it added:

- An in-process unit test,
  `closure_captures_serialize_deterministically_regardless_of_insertion_order_issue_10051`
  (`precompile.rs`), mirroring the existing
  `method_tables_serialize_deterministically_with_typed_key_issue_9197_s7` —
  `closure_captures`' sortedness was previously pinned only indirectly by the
  slow cross-process integration test.
- Doc comments on `sjulia_precompile_determinism_tests.rs` recording that the
  test compares the **full** cache payload byte-for-byte and must opt out of
  persistent prelude/Base caches. Separate subprocesses draw independent
  `HashMap` seeds, but only after each process is forced through compilation;
  shared persistent artifacts bypass that code and invalidate the premise.
- **Injection self-test** (transcript in PR #10142): temporarily disabled
  the `method_table_entries.sort_by(...)` call in `serialize_base_cache`,
  confirmed `precompile_base_is_deterministic_across_processes` failed with
  "produced different bytes for the same prelude across two independent
  processes", then reverted (`git diff` clean) and confirmed green again —
  proof the regression guard is live, not just passing by accident.

## Package Loader Cache (`loader.rs`, Issue #7921)

The package loader (`subset_julia_vm/src/loader.rs`) keeps a **separate**
persistent cache from the Base/Program caches above: one lowered `Module` per
loaded package, written as `<sanitized-name>.<source-hash>.ji.json` under
`SUBSETJULIA_CACHE_DIR` (default `$TMPDIR/subset_julia_vm_cache`). This is the
cache that backs `using AbstractAlgebra` / `using MacroTools` etc.

`UsingImport.span.definition_order` records the evaluation event that caused a
load. `PackageLoader::load_into_program` recursively composes each fresh or
restored package fragment at its package-local import anchors, then inserts the
fragment at the caller's anchor through
`DefinitionOrderCursor::insert_fragment_after`, shifting only later
definitions. The compiler uses those ordinals to resolve same-signature
redefinitions; distinct overlapping signatures remain ordered by structural
specificity, independently of collection/global-index order. Rebasing recursively
updates both stored definitions and their executable
copies inside module/main/function/macro/inner-constructor blocks; leaving a
`Stmt::FunctionDef` copy fragment-local can turn an earlier type annotation into
a false forward reference (Issue #11144). This is part of the semantic snapshot
contract: a cache hit must preserve
evaluation chronology, not merely deserialize an equivalent vector shape
(Issues #11036/#11128/#11144; related semantic-snapshot work #10462). Directly pushing
a cached Module onto `Program.modules` is forbidden by
`check_definition_order_merges.sh`.

The persisted Core-IR version is 5. Versions 4 and earlier are rejected rather
than replayed because they can contain package modules composed with the old
whole-program append chronology.

A cache entry (`CachedModule`) is validated against, in `read_cache`:

- `version` (`loader.rs::CACHE_VERSION`, distinct from the Base `CACHE_VERSION`)
- `vm_version` (`CARGO_PKG_VERSION`)
- `target` (`os-arch`)
- `schema_fingerprint` — a SHA-256 of the JSON of a canonical probe `Module`
- `module_name`
- `source_hash` — SHA-256 of the package source tree (`Project.toml` + `.jl`s)

Validated fresh and restored `Module`s share one post-load reconstruction pass
before `PackageLoader.loaded` is mutated. It recursively registers qualified
struct, abstract-type, and primitive-type families from Core IR, including
nested-module owner paths (Issue #11280). This state is derived rather than
serialized: the thread-local nominal registry is cumulative across packages on
the loader thread, while each cache entry owns only its own declarations.
Registration happens after dependency loading succeeds, so a failed package
load cannot leak partial nominal state.

The other lowering thread-locals do not cross this boundary. Type-alias,
runtime-type-binding, and declared-type tables are snapshot/restored around a
lowering pass; binder frames and quote/generated-unquote flags are lexical
guards; name-conversion caches are pure memoization. The nominal registry is
the durable post-lowering side effect consumed by later compilation, so it is
the only state reconstructed here. Type aliases remain serialized `Module`
bindings and are intentionally not registered as nominal declarations.

This reconstruction does not require a package `CACHE_VERSION` bump: the
serialized `Module` shape and lowering semantics are unchanged. The loader now
derives a missing thread-local side effect from an already validated payload.

**Why `source_hash` is not enough (the #7921 bug):** `source_hash` tracks only
the package *source*, not the lowering/metadata that produced the cached
`Module`. When the lowered `Module` metadata shape changed (it gained
type-alias / module-binding entries such as `PolynomialElem`, `MatrixElem`)
without a `CACHE_VERSION` bump, an older `.ji.json` on the same source was
silently reused — so `isdefined(AbstractAlgebra, :PolynomialElem)` was `false`
from the default cache but `true` from a fresh `SUBSETJULIA_CACHE_DIR`.

**Two-layer invalidation:**

1. `CACHE_VERSION` (manual): bump it whenever the serialized `Module` shape or
   semantics change, including the meaning of existing fields such as stamped
   definition-order spans. This invalidates pre-existing stale entries immediately.
2. `module_schema_fingerprint()` (automatic): hashes a probe `Module` whose
   collections include one representative `TypeAliasDef`. Serde emits every
   field name even for empty collections, so adding/removing a top-level
   `Module` field — or reshaping the probed `TypeAliasDef` — changes the
   fingerprint and invalidates stale entries *even if `CACHE_VERSION` is not
   bumped*. This is the safety net for the "forgot to bump the constant" case.

The fingerprint detects serialized **shape**, not every lowering-semantic
change that produces different `Function.body` contents with the same shape.
Such changes still require a manual `CACHE_VERSION` bump; Issue #11154 moved
version 19 to 20 when annotated keyword defaults gained a two-phase
materialization/assertion prologue.

When modifying the cached `Module` shape or `loader.rs::CACHE_VERSION`:

- [ ] Bump `loader.rs::CACHE_VERSION` and add a one-line history note in its doc
- [ ] If the change is to a nested metadata type the fingerprint should track,
      extend the probe in `module_schema_fingerprint()` so the fingerprint moves
- [ ] Verify `loader::tests::test_stale_cache_with_mismatched_schema_is_rejected`
      and `loader::tests::test_cache_roundtrip_hits_with_matching_schema` pass

## Preloaded-Package Bytecode Cache — Necessity Audit (Issue #9876, 2026-07-10)

`subset_julia_vm_compile/src/compile/preload_cache.rs` (Issue #9189/#9230/#9245/#9254/
#9646/#9477) is a **separate, compile-time-configured** cache from everything
above: it splices already-compiled bytecode for bundled-package functions
(`Plots`, `LinearAlgebra`, …) into a compile whose non-Base function layout
matches the layout the cache was generated with. This audit (Issue #9876)
measures whether it is still worth carrying.

### Mechanism recap (see `preload_cache.rs` module doc for the full history)

- **Compile-time gated, off by default.** `PRELOAD_PACKAGES` is
  `option_env!("SJULIA_PRELOAD_PACKAGES")`, baked in at `cargo build` time. A
  plain `cargo build --release -p subset_julia_vm --bin sjulia --features repl`
  (the command every CLI/nextest/WASM build uses) leaves it `""`, and
  `get_or_init_preload_cache()` returns `None` with zero cost. Since Issue
  #10160, `build.sh` also leaves it empty by default; the iOS xcframework build
  only sets it when the operator supplies an explicit non-empty
  `SJULIA_PRELOAD_PACKAGES`. WASM (`wasm-pack`) and CI never set it.
- **Runtime gate**: a `closure_layout` prefix match over the program's whole
  non-Base function region (Issue #9254) plus a struct-`type_id` guard (Issue
  #9646). Any interposed user function, lifted main lambda, or top-level
  struct fails the match and falls back to an ordinary (always-correct)
  compile — fail-safe, never a stale-index dispatch.

### What `build.sh` shipped before Issue #10160

Before Issue #10160, `build.sh`'s `detect_sample_preload_packages()` unioned
the `using`/`import` roots of **every**
`SubsetJuliaVMApp/.../Resources/Samples/**/*.jl` file, in file-scan order,
into one `SJULIA_PRELOAD_PACKAGES` value. On this checkout (`a87e25a2a3`) that
is:

```
AbstractAlgebra,Distributions,Random,StatsPlots,LinearAlgebra,Test,Optim,Primes,Symbolics,Plots,JSXGraph,Interact,StaticArrays,OrdinaryDiffEq
```

`generate_preload_cache_for` compiles that whole 14-package closure as **one**
`using P1\nusing P2\n...\nusing P14` program and stores its `closure_layout`.
The runtime gate requires `all_functions.len() >= base_function_count +
layout.len()` **and** an exact-order match over that whole prefix — i.e. the
*consuming* program must load all 14 packages, in that exact order, before the
cache can splice anything. No shipped sample does that (each uses its own
1–3-package subset), and even the two packages the issue targets load in the
opposite order here (`LinearAlgebra` before `Plots`) from the `sinc_surface.jl`
sample (`using Plots` then `using LinearAlgebra`). **Empirically confirmed
below: the auto-detected config never activates for any real sample shape**.
Issue #10160 removed this default generation/embed path; it can only be
reproduced now by explicitly passing the same union list.

### Measurement setup

Machine: Apple M2 Max, 12 cores, 96 GB RAM, macOS 26.5.1 (Darwin 25.5.0),
quiet (dedicated to this measurement). Commit under test: `a87e25a2a3`
(`origin/main` tip at audit time). All binaries built with the two-stage
cache-embedded procedure (AGENTS.md "Precompiled cache build"):

```bash
RUSTC_WRAPPER=sccache cargo build --release -p subset_julia_vm --bin sjulia --features repl
./target/release/sjulia --precompile-prelude "$(pwd)/target/prelude_program_cache.bin"
./target/release/sjulia --precompile-base "$(pwd)/target/base_cache.bin"

# A — baseline: no SJULIA_PRELOAD_PACKAGES (what CLI/nextest/WASM/CI ship)
SJULIA_PRELUDE_PROGRAM_CACHE=.../prelude_program_cache.bin \
SJULIA_BASE_CACHE=.../base_cache.bin \
  cargo build --release -p subset_julia_vm --bin sjulia --features repl
# -> sjulia_baseline (sha256 b5e1284168803df9a672875faa848f2119a04a86ed37cb36d791fca907611a2a, 40,685,920 bytes)

# B — narrow preload, exact match for workload (a)'s `using` order
./target/release/sjulia --precompile-packages .../preload_cache_narrow.bin "Plots,LinearAlgebra"
SJULIA_PRELUDE_PROGRAM_CACHE=... SJULIA_BASE_CACHE=... \
SJULIA_PRELOAD_PACKAGES="Plots,LinearAlgebra" SJULIA_PRELOAD_CACHE=.../preload_cache_narrow.bin \
  cargo build --release -p subset_julia_vm --bin sjulia --features repl
# -> sjulia_preload_narrow (sha256 13a629a93d4a39644ac01da05b09d65bb472ccc067f599ed28138cc98cb47a2c, 41,643,616 bytes)

# C — former shipped config: build.sh's pre-#10160 auto-detected 14-package union, real order
./target/release/sjulia --precompile-packages .../preload_cache_shipped.bin \
  "AbstractAlgebra,Distributions,Random,StatsPlots,LinearAlgebra,Test,Optim,Primes,Symbolics,Plots,JSXGraph,Interact,StaticArrays,OrdinaryDiffEq"
SJULIA_PRELUDE_PROGRAM_CACHE=... SJULIA_BASE_CACHE=... \
SJULIA_PRELOAD_PACKAGES="AbstractAlgebra,...,OrdinaryDiffEq" SJULIA_PRELOAD_CACHE=.../preload_cache_shipped.bin \
  cargo build --release -p subset_julia_vm --bin sjulia --features repl
# -> sjulia_preload_shipped (sha256 90b3ffbaa78d39e169c20576101f2eb2633b9c13fb4d829df1027113d07ab589, 44,698,336 bytes)
```

Workloads: (a) `using Plots; using LinearAlgebra; println(1)` (the gate-ON
shape); (b) `using Plots; using LinearAlgebra; surface(-1:0.1:1, -1:0.1:1,
(x, y) -> sinc(sqrt(x^2 + y^2)))` (main-lambda Surface-sample shape, #9158/
#9477 gate-OFF proxy); (c) `using LinearAlgebra; println(1)` (single-package,
order-mismatched against the B/C caches); (d) `-e '1+1'` (floor). 9 cold runs
per (binary, workload) pair (independent process invocations); table reports
median with (min–max).

### Results — cold CLI wall time, ms (9 runs, median [min–max])

| Workload | A baseline (preload inert) | B narrow preload (`Plots,LinearAlgebra`) | C former shipped preload (14-pkg union) |
|---|---:|---:|---:|
| (a) `using Plots; using LinearAlgebra; println(1)` | 416.29 [415.46–419.03] | **111.41 [110.65–114.69]** | 420.18 [419.74–423.90] |
| (b) Surface main-lambda | 432.48 [429.73–435.27] | 432.37 [431.10–439.14] | 436.62 [434.52–440.86] |
| (c) `using LinearAlgebra` only | 192.65 [191.33–195.52] | 193.95 [193.34–197.94] | — |
| (d) `-e '1+1'` floor | 57.75 [57.49–58.45] | 59.14 [58.82–59.26] | — |

Output was verified **byte-identical across A/B/C for every workload**
(fail-safe correctness holds).

### `SJULIA_COMPILE_PROFILE=1` attribution, workload (a)

| binary | `compile_with_cache` wall | `compile.emit_functions` | share |
|---|---:|---:|---:|
| A baseline | 367.9 ms | 312.2 ms | 84.9% |
| B narrow preload (gate ACTIVE) | 63.3 ms | 7.6 ms | 12.0% |
| C former shipped preload (gate inactive) | 372.7 ms | 312.9 ms | 84.0% |

With the gate active, `emit_functions` (inference+codegen) collapses from
~312 ms to ~7.6 ms; `build_method_tables`/`method_table_setup` (the gate
lookup + splice bookkeeping itself) become the new leading cost at ~15.8 ms +
~22.9 ms. Workload (b) and (c) show **no `emit_functions` change** under any
binary — the gate does not fire for either shape (main lambda / package-set
mismatch), matching the wall-clock table.

### Findings

1. **Zero benefit to any current CLI/nextest/WASM/CI/default-iOS build.** The mechanism
   is compile-time-gated off unless `SJULIA_PRELOAD_PACKAGES` is explicitly
   set. After Issue #10160, `build.sh` no longer sets it by default.
2. **The former shipped iOS default had zero benefit.** `build.sh`'s old
   auto-detected 14-package union could structurally never match any single
   real sample's `using` prefix (package-set and order both diverged) —
   confirmed empirically above (row C ≈ row A on every workload, plus a small
   ~4 ms embedded-cache-decode tax on (a) for zero return). Issue #10160 stops
   embedding that dead cache by default.
3. **Zero benefit to the flagship motivating case even with a hand-tuned
   package list.** Workload (b) — the #9158 Surface sample's exact shape,
   `surface(x, y, (x,y) -> ...)` after `using Plots; using LinearAlgebra` — is
   the case the cache was built for, and its main lifted lambda deactivates
   the gate (Issue #9477, still open). Rows B and A are statistically
   indistinguishable on (b).
4. **Real (~73%, ~305 ms) benefit exists, but only for a narrow shape**: a
   bare `using P1; using P2; ...; <no lifted lambda, no top-level user
   function/struct>` program whose package list and order are hand-configured
   to match exactly. No shipped sample or default build reaches this shape
   today.
5. **The mechanism has caused two silent-wrong-output bugs** in production
   history (Issue #9254 — 2-D line instead of a 3-D surface; Issue #9646 — a
   user struct silently corrupting `typeof(lu(A))` to `Plots.AnimatedGif`),
   both from the same class of frozen-index invalidation that the current
   fail-safe gates only partially cover (per #9477, the struct-constructor
   region is not the last surprise; a future gap in the same family is
   plausible).
6. Base cache (#9250) already keeps the *floor* for main-lambda/anonymous
   top-level-value programs far below the pre-#9250 no-cache baseline (the (b)
   row here, ~432 ms, is with Base cache active — pre-#9250 the same program
   compiled from scratch would be several times slower). The preload cache's
   marginal prize on top of that floor is the (a)-style ~305 ms gap, and only
   for programs that already avoid a lifted lambda.

### Recommendation: narrow, do not restore/extend (Issue #9876)

**Narrow the preload cache to the one shape it actually helps** (a
non-lambda, non-struct, exact-package-list `using` prologue) and stop
carrying build/maintenance cost for shapes it cannot reach:

- **`build.sh`'s auto-detected union config should not be used as-is** — it
  was dead weight (finding 2). Issue #10160 implemented the pragmatic fix:
  default iOS builds keep `SJULIA_PRELOAD_PACKAGES` empty and skip preload
  generation/embed; an explicit, deliberately-chosen package list is required
  for a specific target sample.
- **#9477** (relocate all user-derived functions incl. the struct-ctor region
  after all deterministic functions, so the gate survives a main lambda) is
  the change that would make the cache reach its actual target (#9158
  Surface). It remains a legitimate, high-value follow-up **if** someone
  picks it up — the ~305 ms prize (finding 4) is real — but it is a deep,
  previously-attempted-and-reverted layout reorg (see
  `memory/project/project_9189_preload_cache_main_lambda_struct_ctor_trap.md`)
  with a demonstrated silent-corruption failure mode, not a quick fix. Given
  finding 5 (two production wrong-output incidents from this exact
  mechanism), do not restore/extend it without also hardening the gate's
  invariant coverage (a generalized "any index space a spliced body can
  reference must be layout-checked" audit, not another one-off patch for the
  struct-ctor region specifically).
- **#9256** (the narrower "relocate trailing Base closures" plan) is
  superseded by #9477's finding that it reintroduces the #9254 bug class; keep
  it closed/superseded in favor of #9477's broader fix.
- **#9395** (Base-cache lazy per-function decode) is an orthogonal, unrelated
  win (decode share of *Base* cache, not the preload-package cache) and is
  unaffected by this audit either way.
- Do **not** drop the mechanism outright yet: its infrastructure (whole-closure
  generation, layout-identity gate, embed plumbing) is sound where it applies,
  well-tested (`preload_cache.rs` unit tests), and inert (zero cost) for every
  build that doesn't opt in — carrying it costs no CLI/WASM/CI runtime today.
  The pragmatic action is to fix the auto-detected `build.sh` config (finding
  2) so the iOS binary at least stops paying decode cost for zero benefit,
  and leave #9477 as an explicit, correctly-scoped follow-up rather than
  silently reactivating it with the current struct-ctor gap.

## Bundled iOS Sample `.sjvmbc` (Issue #9945)

`./build.sh` precompiles every bundled iOS sample
(`SubsetJuliaVMApp/SubsetJuliaVMApp/Resources/Samples/**/*.jl`) to a sibling
`<basename>.sjvmbc` via the host `target/release/sjulia --compile-vm` (format:
`subset_julia_vm/src/vm_bytecode_file.rs` — `MAGIC "SJVM"` + format version +
bincode `Program`+`CompiledProgram`). The app directory is an Xcode
filesystem-synchronized group, so the generated files ship in the app bundle
automatically next to the `.jl` resources; they are gitignored build
artifacts, never committed. `./build.sh --samples-bytecode-only` regenerates
them standalone (no Xcode required; strict — non-zero exit on any compile
failure), while the full build flow is best-effort per the #9945 acceptance
criteria (`SJULIA_SAMPLES_BC_STRICT=1` opts into strict there).

Invalidation contract:

- A sample's `.sjvmbc` is regenerated whenever the sample `.jl` or the host
  `sjulia` binary is newer than it; build.sh builds that binary from the same
  source tree as the iOS xcframework, so the bundled (`.jl`, `.sjvmbc`) pair
  always matches the VM embedded in the app.
- Orphaned `.sjvmbc` (sample removed/renamed) and outputs of failed compiles
  are deleted so a stale cache never ships.
- **Runtime loader (Issue #10171)**: when a bundled sample is run with its
  pristine body (editor buffer byte-identical to the `.jl` resource), the iOS
  app executes the sibling `.sjvmbc` through the C ABI
  (`run_vm_bytecode_streaming` / `run_vm_bytecode_detailed` in
  `subset_julia_vm_ffi/src/bytecode.rs`, backed by
  `vm_bytecode_file::load_from_bytes`) instead of compiling the source.
  Editor-modified code never qualifies. ANY `VmBytecodeFileError`
  (magic/version/fingerprint/deserialize) is reported as the distinct
  `CErrorKind_StaleBytecode` status and the
  host (`VMBridge.executeStreamingPreferringBytecode`) silently falls back to
  compiling the `.jl` source; a stale payload is never a user-visible error.
  Execution errors after a successful load also fall back (the bundled
  bytecode must never behave worse than source compilation), except user
  cancellation, which is returned as-is so Stop is not defeated.
- The `.sjvmbc` header carries an exact format version plus Base-cache schema,
  compiler-build, and enum-variant fingerprints (Issue #10170). The loader
  rejects both older and newer versions, as well as any fingerprint mismatch,
  before decoding the positional payload; the #10171 host path treats those
  stale-artifact errors as a source-recompile fallback.

## Cache-Restore Parity Invariant (Issue #10265, P0)

**Invariant: a compile context rebuilt on a cache/serialization boundary must
reproduce EXACTLY the compile context the fresh compile pipeline builds.
Defaulted reconstruction (`false`, `HashMap::new()`, `Vec::new()`, …) of a
field the fresh path populates is forbidden unless it carries an in-code
`(Issue #NNNN)` justification AND a matching exemption in the parity guard.**

Why this is a P0 rule and not a style preference: `CompiledProgram.
compile_context` is `#[serde(skip)]` (Issue #3973), so every serialized cache
(persistent/embedded Base cache, `.sjvmbc` files) must *rebuild* the
`RuntimeCompileContext` at load time. Issue #10092 was exactly a defaulted
rebuild: both Base-cache restore paths
(`pipeline_ctx.rs::build_struct_tables`'s cached branch and
`cache.rs::restore_compile_context_from_program`) hardcoded
`has_inner_constructor: false` because the serialized `StructDefInfo` does not
carry the flag. The compiler then synthesized the field-count default
constructor for Base structs that suppress it — `WeakRef(x)` stopped routing
through `WeakRef(x) = _weakref_new(x)`, weak cells were never registered with
the GC, and `GC.gc()` could not clear standalone WeakRef targets. Fixed by
recovering the flag from the IR (`collect.rs::collect_inner_constructor_flags`
/ `inner_constructor_flag_for`, PR #10306); generalized here.

### Restore-path inventory (2026-07-11 audit, Issue #10265)

Restore sites: `cache.rs::restore_compile_context_from_program` (called by
`restore_base_compile_context` for Base-cache hits and by
`vm_bytecode_file.rs::load` for `.sjvmbc`), and
`pipeline_ctx.rs::build_struct_tables`' `precompiled_base` branch. Field-by-
field status against the fresh construction
(`pipeline_ctx.rs` final assembly):

| Field | Restore source | Status |
|-------|----------------|--------|
| `struct_table.*.type_id` | positional index over `compiled.struct_defs` | safe — cached bytecode's `NewStruct` ids are defined by that order |
| `struct_table.*.is_mutable` | carried (`StructDefInfo.is_mutable`) | safe |
| `struct_table.*.fields` | carried (`StructDefInfo.fields`) | safe |
| `struct_table.*.has_inner_constructor` | recovered from IR (PR #10306) | **fixed** (was the #10092 bug: hardcoded `false`) |
| `struct_table` bare-name aliases for module structs | rebuilt by walking IR module structs in fresh order (`SpinLock` → `Threads.SpinLock`, #10078 clobber ordering) | **fixed in #10265 PR (Issue #10337)** — restore previously lost every bare alias; found empirically by the parity guard. The Base-cache **compile lane** had the same gap in `build_struct_tables`' skip branch (bare aliases and parametric short names not registered for cached module structs, so bare-name resolution was cache-mode-dependent) — **fixed in #10265 PR (Issue #10341)**, found by the fresh-vs-cached guard |
| `struct_defs` | carried | safe |
| `parametric_structs` | rebuilt from IR: top-level + qualified AND bare module names, `parent_type` module-qualified | **fixed in #10265 PR (Issue #10337)** — restore previously skipped bare aliases and parent qualification |
| `type_aliases` | rebuilt from IR, **prelude aliases registered first** (matches fresh, Issue #5065 shape) | **fixed in #10265 PR (Issue #10336)** — restore previously skipped prelude aliases on the `.sjvmbc` path |
| `inference_global_types` | persisted as sorted `CompiledProgram::inference_global_types_snapshot`, then collected into the transient context | **fixed (Issue #10333)** — both whole-program serde (`.sjvmbc`/manual restore) and the sectioned Base-cache format carry the already-finalized fresh map, including precise const types and widened mutable globals |
| `primitive_types` | carried (`compiled.primitive_types`) | safe (Issue #5058) |
| `disable_array_getindex_specialization` | persisted after fresh method-table detection as `CompiledProgram::specialization_disable_flags.array_getindex`, then copied into the transient context | **fixed (Issue #10334)** — whole-program serde (`.sjvmbc`/manual restore) and the sectioned Base-cache format carry the finalized decision; restore no longer loses module-owned overrides or alias-typed receivers in an IR rescan |
| `disable_array_setindex_specialization` | persisted after fresh method-table detection as `CompiledProgram::specialization_disable_flags.array_setindex`, then copied into the transient context | **fixed (Issue #10334)** — the same finalized fresh decision is restored exactly across the context-rehydration lanes |
| `disable_field_access_specialization` | persisted after fresh method-table detection as `CompiledProgram::specialization_disable_flags.field_access`, then copied into the transient context | **fixed (Issue #10334)** — module-owned `getproperty` overrides now produce the same disable decision before and after restore |
| `module_registry` | rebuilt by walking `program.modules` in the same depth-first order the fresh path uses (`collect::register_module_ids`) | **safe by construction** (Issue #10988) — derived, not persisted; see "Owner-scoped id relocation pattern" below |
| context presence (`Some`/`None` trigger) | `program_needs_restored_compile_context` | mirrors the fresh trigger; prelude-alias registration keeps the `.sjvmbc` path in `Some` like fresh |

Adjacent restore boundaries audited at the same time:

- `cached_base_from_serialized` (Base cache): `method_tables`,
  `closure_captures`, `promotion_rules` carried; `inference_results`
  deliberately dropped with an in-code justification (Issues #6348/#6495) —
  a compliant, documented exemption.
- **Seeded PROGRAM_CACHE** (Issue #10120): a decoded hit is passed through
  `restore_compile_context_from_program` with the caller's live `Program`
  (the same restore entry point the Base-cache and `.sjvmbc` lanes use), so a
  seeded hit carries the same compile context a fresh compile of the identical
  source would — **fixed (Issue #10335)**; guarded by
  `seeded_program_cache_hit_restores_compile_context_10335`, which injects a
  serialized entry and drives the real lookup.
- **`.sjvmbc` non-context hydration** — **fixed (Issue #10339)**: the
  format-v7 payload records the compiling process's promotion rules (sorted),
  and `vm_bytecode_file.rs::load` replays them + `mark_registry_initialized`
  after deserialize — the same hydration `cached_base_from_serialized`
  performs — so reflection-visible registry state no longer diverges on the
  `.sjvmbc` execution path (guarded by `load_replays_promotion_rules_10339`).
  The other `#[serde(skip)]` fields:
  `CompiledProgram.main_scope_names` (Issue #9182; REPL-only consumer, empty
  after any decode — no `.sjvmbc`-CLI consumer, still latent should a decoded
  program ever reach a REPL session) and `FunctionInfo.shared_plan`
  (Issue #9089; intentionally runtime-only, documented in-code — compliant
  exemption).
- **GC-root hypothesis (issue #10265 root-cause 2) — investigated, does not
  hold**: serialized caches contain bytecode + metadata (plus `Rc` `Value`
  *constants* inside instructions, which exist identically in a fresh
  compile). No runtime heap values are serialized or pinned by the cache; the
  observed WeakRef pinning was fully explained by the constructor-flag loss
  above. Cache/GC-root separation needs no further mechanism today.
- **Known scope limit of the bare-alias reconstruction** (codex finding 4):
  the restore walk covers `program.modules`; modules appended at compile time
  by `load_stdlib_modules` (JSON-IR lane only — `parse_and_lower` already
  folds `using`-loaded packages into `Program.modules`) are not walked. If a
  serialized program ever carries struct-bearing stdlib modules outside
  `Program.modules`, their bare aliases would be missing; the parity guard's
  corpus is the place to encode that case when it becomes reachable.

### The guard (`cache.rs` unit tests, runs in `--lib`)

- `restored_compile_context_matches_fresh_compile_10265` — compiles a
  representative corpus fresh, round-trips the `CompiledProgram` through the
  REAL cache serializer (dropping the context), rebuilds it with the REAL
  restore entry point, and asserts field-by-field parity. The comparison
  **exhaustively destructures** `RuntimeCompileContext` and `StructInfo`
  (no `..`), so adding a field to either type fails compilation inside the
  guard until the restore story is decided — the compile-error-shaped half
  (precedent: #10060's exhaustive match).
- `base_cached_compile_struct_table_matches_fresh_compile_10265` — compiles
  the same corpus fresh vs through the Base-bytecode-cache lane
  (`build_struct_tables`' cached branch, the lane #10092 regressed on) and
  asserts the struct tables agree on every entry modulo lane-local
  `type_id`s (ids are checked structurally against each program's own
  `struct_defs`).

Rules when you touch this area:

1. Adding a field to `RuntimeCompileContext` / `StructInfo`: the guard stops
   compiling. Make every restore path reproduce the field (serialize it, or
   recover it from the IR like #10306), then extend the parity assertions.
   Only if reproduction is impossible may you add an exemption — with a filed
   Issue number, an in-code comment, and an assertion pinning the *current*
   restored value so a later fix must revisit the exemption.
2. Never hardcode a fresh-path-populated value in a restore path without the
   same Issue-tracked exemption treatment.
3. Cache-mode-dependent *runtime* behavior (the #10092 symptom level) is the
   subject of the dual-cache-mode fixture lane tracked in Issue #10223; this
   section's guard catches the compile-context divergence class before it
   reaches runtime.

## Owner-scoped id relocation pattern (Issue #10988 Phase 2a)

`docs/vm/SEMANTIC_ID_MIGRATION.md` (Issue #10459 Phase 0) is retiring
bare-name identity `HashMap<String, _>` tables in favor of typed, owner-scoped
ids (`ModuleId` now; `StructId`/`FunctionId` in Phase 2b/3). Every phase that
introduces a typed id for a domain with cache-serialized sites must answer:
*how does this id survive a cache/serialization boundary, and what happens
when a cached payload predates the id?* Phase 2a (`ModuleId`,
`subset_julia_vm_bytecode::module_intern`) is the first phase to face this and
establishes two DISTINCT patterns — pick whichever matches your domain's
actual wire-format shape, not the one that sounds more sophisticated:

### Pattern A — derive, don't persist (used by `RuntimeCompileContext::module_registry`)

Applies when the id's *source data* is itself part of the wire format and
already round-trips faithfully (e.g. the `Program`'s `Module` AST — `name` +
`submodules`, walked in a stable depth-first order). Here the id table is
**not serialized at all**: it is rebuilt by re-walking that structural source
in the *exact same deterministic order* the fresh-compile path used
(`compile/collect.rs::register_module_ids`, called from both the fresh
`collect_module_metadata` path and the restore path
`compile/cache.rs::restore_compile_context_from_program`). Because both call
sites are literally the same function over the same (possibly
cache-round-tripped) AST, fresh-compile and cache-restore always allocate
identical ids for identical module paths — no persisted counter, no
invalidate-on-mismatch check, no `CACHE_VERSION` bump needed for *this*
specific field. Regression coverage:
`restored_compile_context_matches_fresh_compile_10265` (`compile/cache.rs`)
asserts `fresh_module_registry`/`restored_module_registry` agree path-for-path
(extended for Issue #10988); `same_name_different_module_gets_distinct_and_stable_ids_issue_10988`
(same file) pins the property end-to-end for two sibling same-named
submodules (`A.Sub`/`B.Sub`).

**When Pattern A applies**: the id-bearing table is a projection of
`RuntimeCompileContext` (or anything else already `#[serde(skip)]` and
rebuilt every compile from IR/AST that itself round-trips through the cache).
Verify the "already round-trips" premise explicitly — do not assume it; Phase
2a found this true for the module tree and confirmed it by reading
`restore_compile_context_from_program` and the prelude Program cache
(`pipeline.rs::PRELUDE_PROGRAM_CACHE_VERSION`) rather than asserting it.

### Pattern A, second consumer — `StructId` / `struct_table` (Issue #11078 Phase 2b)

Phase 2b re-keyed `SharedCompileContext::struct_table` /
`RuntimeCompileContext::struct_table` from `HashMap<String, StructInfo>` to a
`StructRegistry` keyed by `StructId { module: ModuleId, local }`, and it lands
squarely in **Pattern A** — which is worth recording, because the Phase 2b issue
(#11078) and the Phase 0 plan both PREDICTED Pattern B ("the largest
cache-relocation surface in the whole epic", "`StructInfo`'s wire format needs
an explicit ID field and relocation"). That prediction was wrong, and the check
that disproves it takes one grep: **`StructInfo` derives no `Serialize`**, and
`RuntimeCompileContext` is `#[serde(skip)]` on `CompiledProgram` (#3973). The
struct table never crosses the wire; it is *rebuilt* on both lanes
(`build_struct_tables` fresh, `restore_compile_context_from_program` on restore).
So: no persisted id, no relocation table, no new invalidate-on-mismatch check.

The Pattern A premise ("the id's source data round-trips faithfully") holds via
`CompiledProgram::struct_defs` (serialized) plus the module tree — but Phase 2b
adds a caveat Phase 2a did not have, and it is the trap to avoid if you extend
this to another domain:

> **The two lanes do NOT register in the same order.** The cached lane seeds
> every cached `struct_defs` entry — parametric instantiations like
> `Complex{Float64}` included — up front, while the fresh lane creates those much
> later, after the user's own structs. Pattern A's "re-walk the same structural
> source in the same order" therefore does NOT hold for struct REGISTRATION order
> the way it does for the module tree.

Hence `StructId`'s two order-independent inputs: `local` is the already
cache-stable dense concrete-type index (`StructInfo::type_id`, the #10265
invariant), NOT a fresh per-module counter; and the owner `ModuleId` comes from a
module table SEEDED by `register_module_ids`' walk *before any struct is
registered* (both lanes). Regression coverage:
`base_cached_compile_struct_table_matches_fresh_compile_10265` (`compile/cache.rs`)
now also asserts a `struct_id_snapshot` — every name resolves to the same
owner-scoped `StructId` fresh vs. restored — and
`unseeded_module_interning_is_registration_order_dependent_issue_11078`
(`struct_registry.rs`) pins the negative control, so nobody removes the seeding
without a red test.

### Pattern B — explicit persisted relocation table (used by `CompiledProgram::macro_bindings`/`module_registry`)

Applies when a table is a **genuinely bincode-serialized** field of
`CompiledProgram` (part of `SerializedBaseCache`'s whole-struct paths —
`.sjvmbc` via `vm_bytecode_file.rs`, the prelude Program cache — or a future
domain's equivalent). Here the id table travels ON the wire, explicitly,
alongside the table it keys: `CompiledProgram.macro_bindings` re-keyed from
`HashMap<String, HashSet<String>>` to `HashMap<ModuleId, HashSet<String>>`,
with a new sibling field `CompiledProgram.module_registry: ModuleInternTable`
serialized right next to it — the "explicit ID field in the persisted wire
format" `docs/vm/SEMANTIC_IDENTITIES.md`'s required property #2 calls for. A
pre-migration cache has no `ModuleId` keys and cannot be reinterpreted under
the new shape (Issue #10459 Phase 2's "never reinterpret a compile-pass-local
numeric ID in a later table"), so `CACHE_VERSION` is bumped (136 -> 137) —
matching the invalidate-on-mismatch contract every persisted cache format in
this document already follows: a version/fingerprint mismatch is a clean
cache miss (recompile from source), never a partial or best-effort decode.

**Important corollary found while auditing this domain**: not every
lexically-inside-`compile/cache.rs`/`compile/precompile.rs` site is actually
Pattern B. The **persistent/embedded Base cache's own section format**
(`compile/precompile.rs::append_compiled_program_section`/
`deserialize_compiled_program_body`) serializes `CompiledProgram` field-by-
field as explicit named sections and never had a `macro_bindings` section at
all — it is always reset to empty on that specific boundary and rebuilt fresh
by the compile pipeline for whatever program reuses the Base prefix (Pattern
A in spirit, on a boundary that happens to share a Rust type with a Pattern-B
field elsewhere). Read the ACTUAL (de)serialize code path for your domain's
table before assuming "lexically declared near `compile_context`" implies
"travels on this wire" — `docs/vm/SEMANTIC_ID_MIGRATION.md`'s own scope
disclaimer makes exactly this point about its mechanical inventory.

### Applying this to Phase 2b (`StructId`) / Phase 3 (`FunctionId`/`MethodId`)

`struct_table`/`method_tables` are the harder case foreshadowed above:
`struct_table` is rebuilt from `compiled.struct_defs` (a REAL, Pattern-B
serialized `Vec<StructDefInfo>`) via a **compile-pass-local positional index**
(`type_id: usize` = the def's position in that vector) — not a stable AST
walk. Two independently-serialized `struct_defs` vectors (e.g. a cached Base
prefix's vs. a fresh recompile's) are NOT guaranteed to index identically, so
`StructId`/`FunctionId` will need a REAL Pattern-B relocation table (an
explicit serialized id <-> definition-index map, not a re-derivable walk) —
this is why `docs/vm/SEMANTIC_ID_MIGRATION.md` calls `struct_table`'s
`StructInfo` wire format "the largest cache-relocation surface in the whole
epic." Confirm which pattern applies per table before implementing; do not
assume Pattern A generalizes just because it worked for `ModuleId`.

## Related Files

| File | Role |
|------|------|
| `subset_julia_vm_compile/src/compile/cache.rs` | `BASE_CACHE`, `PROGRAM_CACHE`, `PROGRAM_CACHE_SEEN`, `CachedBase`, `clear_cache()`, `get_or_init_base_cache()` |
| `subset_julia_vm/src/promotion.rs` | `PROMOTION_RULE_REGISTRY`, `clear_registry()`, `is_registry_initialized()` |
| `subset_julia_vm_compile/src/compile/precompile.rs` | Serialize/deserialize `SerializedBaseCache` (includes `promotion_rules`, `inference_results`) |
| `subset_julia_vm_compile/src/compile/embedded_cache.rs` | Load embedded precompiled cache at startup |
| `subset_julia_vm/src/pipeline.rs` | Load persistent/embedded prelude Program cache before `parse_and_lower()` merges Base with user code |
| `subset_julia_vm/src/loader.rs` | Package loader `.ji.json` cache: `CachedModule`, `CACHE_VERSION`, `module_schema_fingerprint()`, `read_cache()`/`write_cache()` (Issue #7921) |
| `subset_julia_vm_compile/src/compile/preload_cache.rs` | Preloaded-package bytecode cache: `PRELOAD_PACKAGES` (compile-time, off by default), `closure_layout` gate, `get_or_init_preload_cache()` (Issue #9189/#9245/#9254/#9646; necessity audit #9876) |

## InferenceCacheKey (Issue #3510)

The interprocedural type-inference engine uses a typed cache key that
mirrors Julia's `MethodInstance` / `WidenedArgtypes` treatment from
`julia/Compiler/src/inferenceresult.jl`.

```rust
// subset_julia_vm_compile/src/compile/abstract_interp/engine/cache_key.rs
pub struct InferenceCacheKey {
    pub fn_id: String,                 // callee identity
    pub argtypes: Vec<CacheArgType>,   // per-arg slot
}

pub enum CacheArgType {
    Type(LatticeType),    // widened (default)
    Const(ConstValue),    // preserved for specialization
}
```

`InferenceCacheKey::new(fn_id, &[LatticeType])` applies
`widen_argtypes_for_cache_key`, which keeps `Const` only for slots that
satisfy `is_const_eligible`. Everything else is widened to its
[`LatticeType::Concrete`] form so calls that differ only in non-eligible
const values share a single inference result.

### Const eligibility policy

Mirrors Julia's "is_specialized_call / is_aggressive_constprop" filter —
preserve only constants that are likely to influence inference:

| `ConstValue`            | Kept as `Const` | Reason |
|-------------------------|------------------|--------|
| `Bool`                  | yes              | Branch elimination (`if flag`) |
| `Symbol`                | yes              | Field access / `Val`-like dispatch |
| `Nothing`               | yes              | Singleton type |
| `Int64`, abs ≤ 8        | yes              | `Val{N}` / tuple-length dispatch |
| `Int64`, abs > 8        | **widened**      | Avoid cache blowup |
| `Float64`               | **widened**      | Rarely affects dispatch |
| `String`                | **widened**      | Population would be unbounded |

The `SMALL_INT_CONST_THRESHOLD` constant (currently `8`) bounds the
small-int policy. `i64::MIN.checked_abs()` is `None`, so the most-negative
integer is correctly widened (not aliased back to itself via wrap-around).

### Behavioral guarantees

- `f(1_000_000)` and `f(2_000_000)` reuse one cache entry.
- `f(1)` and `f(2)` get distinct entries (small-int specialization on).
- `f(true)` and `f(false)` get distinct entries (branch elimination).
- The inference engine's cache and `analyzing_functions` (recursion-cycle
  detection) both key on `InferenceCacheKey`, so cycle detection inherits
  the same widening.

### AoT migration

The AoT inference engine in `subset_julia_vm/src/aot/inference/engine/`
still uses an ad-hoc `(name, Vec<StaticType>)` shape because AoT operates
on `StaticType`, not `LatticeType`. The migration to `InferenceCacheKey`
is tracked in #3510 as a follow-up so the VM-side change can land
independently.

## Related Issues

| Issue | Description |
|-------|-------------|
| #2489 | `show_methods` lost in cache (same structural pattern) |
| #3025 | Promotion registry always empty (wrong extraction site) |
| #3036 | Promotion registry not restored on second `compile_with_cache()` |
| #3038 | Prevention: `clear_cache()` must also clear all registries |
| #3510 | `InferenceCacheKey` with controlled const specialization |
| #7921 | Package loader cache reused stale `Module` binding/type-alias metadata; fixed by `loader.rs::CACHE_VERSION` bump + `module_schema_fingerprint()` |
| #8626 | Enum variant fingerprint in cache headers so inserting/removing/reordering `Instr`/`BuiltinId`/`Intrinsic`/`BuiltinOp` variants is detected and triggers regeneration instead of silent misdecoding |
| #8627 | Wire ID table (`compile/instr_wire_ids.rs`) decouples `BuiltinId`/`Intrinsic`/`BuiltinOp` bincode serialization from declaration order; initial IDs = current declaration indices for byte-compatibility; future reorders/removals update the table and tombstone retired IDs rather than touching the enum body |
| #9189 / #9230 / #9245 | Preloaded-package bytecode cache: mechanism, whole-closure generation, layout-identity gate |
| #9254 | `closure_layout` gate must span the full non-Base region (trailing lifted Base closures), or a main lambda silently corrupts a spliced `surface()` render |
| #9256 | Narrow "relocate trailing Base closures" restoration plan — superseded by #9477's finding that it reintroduces #9254 |
| #9477 | Preload gate still deactivates for the #9158 Surface sample itself (main lifted lambda); the obvious fix regresses via the struct-ctor region — open follow-up |
| #9646 | User top-level struct shifts concrete struct `type_id`s and corrupts spliced `NewStruct` operands; gate now also fails-safe on any top-level struct |
| #9876 | Necessity audit (this section): zero benefit to any current CLI/WASM/CI build (compile-time gated off by default) and to the former shipped iOS auto-union build (`build.sh`'s package union structurally never matched any real sample); recommendation: narrow, do not restore/extend without hardening the gate's invariant coverage |
| #10160 | Fixed: `build.sh` no longer auto-detects a sample-union `SJULIA_PRELOAD_PACKAGES` value by default; explicit package lists are required before generating/embedding a preload cache |
