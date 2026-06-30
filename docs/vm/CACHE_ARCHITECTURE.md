# Cache Architecture in SubsetJuliaVM

This document describes the thread-local state management pattern used during
Base compilation, and the invariants that must be maintained between the Base
cache and associated registries.

## Overview

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

`CompiledProgram.specializable_functions` is also serialized as part of
`SerializedBaseCache.compiled`, so Base runtime specialization targets survive
persistent/embedded cache roundtrips. `CompiledProgram.compile_context` remains
`#[serde(skip)]` and is rebuilt by `cached_base_from_serialized()` so the
serialized payload does not need to include full prelude IR context.

## SerializedBaseCache (Issue #3240)

When modifying `SerializedBaseCache` fields or `CACHE_VERSION`:

- [ ] Increment `CACHE_VERSION`
- [ ] Verify `test_serialize_deserialize_roundtrip_empty_program` passes
- [ ] Verify `test_version_mismatch_returns_error` passes

## Package Loader Cache (`loader.rs`, Issue #7921)

The package loader (`subset_julia_vm/src/loader.rs`) keeps a **separate**
persistent cache from the Base/Program caches above: one lowered `Module` per
loaded package, written as `<sanitized-name>.<source-hash>.ji.json` under
`SUBSETJULIA_CACHE_DIR` (default `$TMPDIR/subset_julia_vm_cache`). This is the
cache that backs `using AbstractAlgebra` / `using MacroTools` etc.

A cache entry (`CachedModule`) is validated against, in `read_cache`:

- `version` (`loader.rs::CACHE_VERSION`, distinct from the Base `CACHE_VERSION`)
- `vm_version` (`CARGO_PKG_VERSION`)
- `target` (`os-arch`)
- `schema_fingerprint` — a SHA-256 of the JSON of a canonical probe `Module`
- `module_name`
- `source_hash` — SHA-256 of the package source tree (`Project.toml` + `.jl`s)

**Why `source_hash` is not enough (the #7921 bug):** `source_hash` tracks only
the package *source*, not the lowering/metadata that produced the cached
`Module`. When the lowered `Module` metadata shape changed (it gained
type-alias / module-binding entries such as `PolynomialElem`, `MatrixElem`)
without a `CACHE_VERSION` bump, an older `.ji.json` on the same source was
silently reused — so `isdefined(AbstractAlgebra, :PolynomialElem)` was `false`
from the default cache but `true` from a fresh `SUBSETJULIA_CACHE_DIR`.

**Two-layer invalidation:**

1. `CACHE_VERSION` (manual): bump it whenever the serialized `Module` shape or
   semantics change. This invalidates pre-existing stale entries immediately.
2. `module_schema_fingerprint()` (automatic): hashes a probe `Module` whose
   collections include one representative `TypeAliasDef`. Serde emits every
   field name even for empty collections, so adding/removing a top-level
   `Module` field — or reshaping the probed `TypeAliasDef` — changes the
   fingerprint and invalidates stale entries *even if `CACHE_VERSION` is not
   bumped*. This is the safety net for the "forgot to bump the constant" case.

When modifying the cached `Module` shape or `loader.rs::CACHE_VERSION`:

- [ ] Bump `loader.rs::CACHE_VERSION` and add a one-line history note in its doc
- [ ] If the change is to a nested metadata type the fingerprint should track,
      extend the probe in `module_schema_fingerprint()` so the fingerprint moves
- [ ] Verify `loader::tests::test_stale_cache_with_mismatched_schema_is_rejected`
      and `loader::tests::test_cache_roundtrip_hits_with_matching_schema` pass

## Related Files

| File | Role |
|------|------|
| `subset_julia_vm/src/compile/cache.rs` | `BASE_CACHE`, `PROGRAM_CACHE`, `PROGRAM_CACHE_SEEN`, `CachedBase`, `clear_cache()`, `get_or_init_base_cache()` |
| `subset_julia_vm/src/compile/promotion.rs` | `PROMOTION_RULE_REGISTRY`, `clear_registry()`, `is_registry_initialized()` |
| `subset_julia_vm/src/compile/precompile.rs` | Serialize/deserialize `SerializedBaseCache` (includes `promotion_rules`, `inference_results`) |
| `subset_julia_vm/src/compile/embedded_cache.rs` | Load embedded precompiled cache at startup |
| `subset_julia_vm/src/pipeline.rs` | Load persistent/embedded prelude Program cache before `parse_and_lower()` merges Base with user code |
| `subset_julia_vm/src/loader.rs` | Package loader `.ji.json` cache: `CachedModule`, `CACHE_VERSION`, `module_schema_fingerprint()`, `read_cache()`/`write_cache()` (Issue #7921) |

## InferenceCacheKey (Issue #3510)

The interprocedural type-inference engine uses a typed cache key that
mirrors Julia's `MethodInstance` / `WidenedArgtypes` treatment from
`julia/Compiler/src/inferenceresult.jl`.

```rust
// subset_julia_vm/src/compile/abstract_interp/engine/cache_key.rs
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
