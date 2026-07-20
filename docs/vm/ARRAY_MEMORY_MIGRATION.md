# Array / Memory Migration Status

*Last updated: 2026-06-17*

This page is the active status note for the Array/Memory migration. The
historical migration log is preserved in
`docs/vm/archived/ARRAY_MEMORY_MIGRATION_HISTORY_20260611.md`.

## Current State

`Value::Array(ArrayRef)` has been retired. Issue #4568 converted
`scripts/check_value_array_allowlist.sh` from a per-file ceiling into a
zero-match audit across `subset_julia_vm/src` and `subset_julia_vm/tests`.

The remaining compatibility carrier is `Value::NativeArray(ArrayRef)`. As of
Issue #6653, public construction, materialization, HOF, broadcast, `similar`,
and `reshape` routes return the Julia-visible `Array{T,N}` wrapper backed by
`MemoryRef{T}`. Host/runtime/cache boundaries use explicit native-array
conversion helpers:

- `native_array_value_ref`
- `native_array_value_mut_ref`
- `native_array_ref_value`
- `native_array_value_from_array`
- `native_array_ref_from_value`

## Carrier removal (Milestone #26 / #6723)

The `Value::NativeArray` carrier is now actively being removed. Sequence:

1. **#6805 (this prep step)** — record a `vm_array_benchmark` baseline
   (`benchmarks/results/vm_array_baseline_6805.md`), add the missing
   micro-benchmark cases (multi-dim index, MemoryRef-backed construction,
   `view`/`SubArray` parent sharing), and extend
   `scripts/check_value_array_allowlist.sh` with a `Value::NativeArray`
   allowlist ratchet (the variant now lives in an explicit 9-file allowlist that
   may only shrink).
2. **#6806** — migrate the VM execution engine (index, locals, calls, dynamic
   dispatch, formatting, builtins) off `Value::NativeArray` to the
   MemoryRef-backed `Array{T,N}` wrapper, one subsystem at a time, re-running
   `cargo nextest run --release`, `bash scripts/test_aot.sh`, and the benchmark
   after each. As files drop their last `Value::NativeArray` use, remove them
   from `NATIVE_ARRAY_ALLOWLIST` (the audit flags stale entries).
   - **Slice 1 (done)**: routed the direct `Value::NativeArray` *variant*
     matches in the four hot-path exec files (`exec/array_index.rs`,
     `exec/call.rs`, `exec/call_dynamic.rs`, `exec/locals.rs`) through the shared
     `native_array_value_ref` destructure helper / a verbatim slot clone
     (`Value::NativeArray(Rc)` clones via a cheap `Rc` bump, so the carrier is
     preserved without an explicit variant match). Behaviour-identical; allowlist
     shrank 9 → 5. Remaining allowlist: the variant definition + an enum-method
     arm (`value/value_enum.rs`), the converter hub (`value/array_value/mod.rs`),
     the carrier unit tests (`frame.rs`), and two doc comments (`formatting.rs`,
     `plotting/mod.rs`).
   - **Slice 2 (done)**: flipped the array *producers* in
     `exec/array_basic.rs` to emit the MemoryRef-backed `Array{T,N}` wrapper
     directly instead of the native carrier — array literals (`PushArrayValue`),
     comprehensions (`FinalizeArray` / `FinalizeArrayTyped`, via a new
     `finalize_top_array_to_wrapper` that converts the finished build buffer),
     and undef constructors (`push_undef_typed_array` →
     `push_array_value_as_wrapper`). The conversion reuses the existing
     `array_wrapper_value_from_array_value` constructor, which copies into a
     `MemoryValue` sharing the `ArrayData` storage. The transient incremental
     build buffer (`NewArray` + `PushElem`) stayed a native carrier here; Slice 4
     de-variants it onto `Value::Memory`. Consumers already handle both
     representations (public constructors return wrappers since #6653), so `main`
     was already mixed-representation; this makes the array *output* uniformly a
     wrapper. The native-carrier typed-slot fast path (`StoreSlotArray` →
     `slot_array`) is now bypassed for wrapper arrays (they route through the
     generic local slot, which already roundtrips them); a wrapper-keyed typed
     fast path / typed+inbounds accessors on `MemoryRefValue` is the next slice
     (PR B). Full suite + AoT green, perf neutral vs the #6805 baseline. No
     allowlist change (these files never matched the variant text).
   - **Slice 3 (done, PR B start)**: gave raw `IndexLoad` a native fast path for
     a rank-1 MemoryRef-backed `Array{T}` wrapper indexed by a single integer
     (`exec/array_index.rs::rank1_memoryref_wrapper_element`), reading the element
     directly from the wrapper's `Memory` instead of dispatching `getindex` per
     index. Untyped-parameter / dynamically-typed array indexing (`f(a)=a[i]`)
     compiles to raw `IndexLoad` and previously routed every access through a Base
     `getindex` method dispatch + interpreter frame; the wrapper is now read
     natively. Gated on the shared #6657 flag
     (`disable_array_getindex_specialization`) so a program defining a user
     `getindex` array override still reaches it via dispatch. Behaviour-identical
     to the materialize-then-`ArrayValue::get` path (the wrapper `Memory` stores
     exactly what `ArrayValue::get` returns) but O(1) and dispatch-free; bounds
     are checked against the logical `shape[0]` so views stay correct. Result:
     `vm_array_benchmark` `hof_broadcast_filter_reduce_128` −38% and
     `view_subarray_parent_share_64` −57% versus the #6805 baseline (both iterate
     wrappers via `getindex`); other cases neutral.
   - **Slice 3b (done, PR B)**: generalized the read fast path to
     multi-dimensional wrappers (`memoryref_wrapper_element`) — it now handles
     both index modes `ArrayValue::linear_index` accepts (a single linear index
     of any rank, or one index per dimension, column-major) and defers other
     arities (e.g. trailing-singleton `v[i, 1]`) to dispatch. `IndexLoadInbounds`
     reads already delegate to `IndexLoad`, so they inherit the fast path
     unchanged. Same #6657 gate and bounds semantics.
   - **Slice 3c (done, PR B)**: native write fast path for `IndexStore` — a
     single-integer write into a MemoryRef-backed numeric `Array{T}` wrapper goes
     directly to its `Memory` instead of dispatching `setindex!` per write. Added
     the `disable_array_setindex_specialization` compile-context flag (mirrors the
     #6657 getindex flag, computed in both `compile/cache.rs` and
     `compile/pipeline_ctx.rs`, recomputed on cache restore — no `CACHE_VERSION`
     bump) so a user `setindex!` array override is still reached via dispatch. The
     store is restricted to value/element pairs where `ArrayData::set_value`
     equals `setindex!`'s `convert(T, v)` — exact-type matches and
     integer-value-into-float-array; every other pair (notably float-into-int,
     which `convert` rounds with an `InexactError` check) falls through to
     dispatch. Result: `vm_array_benchmark` `construction_undef_zeros_128` −46%
     (fill via `setindex!`) and `hof_broadcast_filter_reduce_128` −49% versus the
     #6805 baseline; `index_mutation_push_pop_128` neutral (typed-slot writes were
     already native). Next within PR B: migrate the ~190 `native_array_value_ref`
     consumer sites off the borrow helper (the #6807 blocker).
   - **Slice 4 (done)**: de-varianted the incremental build buffer off the native
     carrier. `NewArray` / `NewArrayTyped` / `PushElem` / `PushElemTyped` /
     `ReserveArray` / `FinalizeArray` / `FinalizeArrayTyped` (`exec/array_basic.rs`)
     now build into a flat, growable `Value::Memory` (the same representation
     `NewMemory` / `MemorySet` use) instead of `Value::NativeArray`, removing the
     **build buffer** as a producer of the carrier (the VM-builtin producers in
     Slice 5 are separate). The build buffer is emitted by the
     lazy specializer for typed array literals (`[1, 2, 3]` → I64/F64/Bool/String/
     Any) and by the empty `Vector{String}` constants (`ARGS` / `DEPOT_PATH` /
     `LOAD_PATH`). `ArrayValue::push`'s ~150-line element logic (Complex interleave,
     Tuple/isbits-struct AoS, normal `push_value`) was extracted to a shared
     `push_into_array_data` routine reused by the new `MemoryValue::push` /
     `push_f64` / `reserve` / `with_capacity` / `is_struct_ref_array`, so the buffer
     grows with identical semantics. `FinalizeArray*` reconstructs the exact
     `ArrayValue` the native buffer held (`memory_first_with_capacity` derives the
     same `struct_type_id` / `element_type_override`; the grown storage and finalize
     shape are swapped in) and converts through the unchanged
     `array_wrapper_value_from_array_value`, so wrapper output is byte-identical.
     This also retired the now-dead shared `native_array_value_mut_ref` converter
     (its only caller was the build buffer). Full suite + AoT green, perf neutral
     vs the #6805 baseline (the element-push path is small literals, not hot loops).
   - **Slice 5 (done)**: flipped the first batch of VM-builtin/instruction array
     producers off the native carrier onto the `Array{T,N}` wrapper via the now
     `pub(crate)` `push_array_value_as_wrapper`: range materialization
     (`MakeRange` / `MakeRangeF64`, `exec/range.rs`), RNG arrays
     (`RandArray` / `RandIntArray` / `RandnArray`, `exec/rng.rs`), and the matrix
     op result (`exec/matrix.rs`). Chosen first because these are fresh
     constructor/transform results — not in the arithmetic/`getindex` dispatch hot
     loops — and analogous to the public `zeros`/`collect` constructors that
     already return wrappers (#6653). **Consumer-readiness fix surfaced here**: the
     native `length` builtin (`builtins_collections.rs`) routed wrapper `StructRef`s
     through `length` method dispatch, which a bare VM (no Base loaded) can't
     resolve. Added a native fallback that counts a MemoryRef-backed wrapper's
     elements directly **only on a dispatch miss** — so a user `length` override
     still wins, and Base-loaded programs (where `length(::AbstractArray)` exists)
     are unaffected. Full suite 3842/3842 (only the bare-VM unit test
     `test_vm_make_range` was exposed, fixed by the `length` fallback), AoT green,
     `vm_array_benchmark` neutral. The remaining ~36 `native_array_value_from_array`
     producer sites (many in the hot binary/index dispatch paths) and the
     paired consumer builtins are subsequent batches.
   - **Slice 6 (done)**: made the wrapper conversion hub
     `array_wrapper_value_from_array_value` copy-free for simple arrays — it
     *moves* the `ArrayData` into the `MemoryValue` instead of an O(n)
     element-copy, when materializing a fresh `undef_typed(element_type)` would
     pick the same storage variant (no element-type/array-type override, no
     shared-parent view, `raw_len == element_count`, primitive backing
     F32/F64/I*/U*/Bool/String/Char). BitPackedBool/StructRefs/Any-family keep the
     materializing copy, so wrapper storage is byte-identical. `construction_undef_zeros_128`
     −0.53%, others neutral. This removes the copy that would make the Slice 7
     native-constructor flip regress construction.
   - **Slice 7 (done)**: flipped the native fresh-array constructor builtins
     (`builtins_arrays.rs`) — `zeros`/`zerosF64`/`zerosI64`, `ones`/`onesF64`/
     `onesI64`, and the `AllocUndef{F64,I64,Bool,Any}` builtins — off the native
     carrier onto the wrapper via `push_array_value_as_wrapper`. The `Mark{BitVector,
     BitArray}` arms (which carry an `array_type_override` and BitPackedBool
     storage) and `Reshape` (which shares parent storage) are intentionally left on
     the carrier — the copy-free fast path would unpack/detach them. Zero blast
     radius: full suite 3842/3842 with no new consumer fixes needed (the Slice 5
     `length` fallback plus the fact that real programs run with Base loaded cover
     the wrapper consumers). `vm_array_benchmark` construction within ~0.3-0.6% of
     the copy-free baseline (mostly noise, within the #6653-accepted migration
     tradeoff), all else neutral.
   - **Slice 8 (done)**: flipped a batch of scattered, non-hot
     `native_array_value_from_array` producers onto the wrapper via
     `push_array_value_as_wrapper`: `readlines`/`readdir`-style file readers
     (`builtins_io.rs`), macro/`eval` array results (`builtins_macro/mod.rs`), and
     reflection results — `return_types`/`methods` (`builtins_reflection/mod.rs`).
     Left for later: `builtins_linalg.rs` (mixes free-function tuple builds for LU
     factorization that lack `struct_heap` access), `type_ops/deep_copy.rs`
     (recursive, drives the widely-used `copy`/`deepcopy`), and `formatting.rs`
     (display/FFI boundary). Zero blast radius: full suite 3842/3842, AoT green, no
     new consumer fixes. Not benched (none of these producers sit on a
     `vm_array_benchmark` path).
   - **Slice 9 (done)**: flipped the linear-algebra result producers
     (`builtins_linalg.rs`) off the native carrier. The file-local
     `linalg_array_value` free function (a thin `native_array_value_from_array`
     wrapper) is replaced by a `Vm::linalg_wrapper(&mut self, ArrayValue)` method
     that builds the MemoryRef-backed `Array{T,N}` wrapper via
     `array_wrapper_value_from_array_value`. Every decomposition consumes its
     input matrices into nalgebra before producing a result, so `self` is free at
     each producer site; all of `lu`/`inv`/`\`/`svd`/`qr`/`eigen`/`eigvals`/
     `cholesky` now return wrappers (19 call sites). The consumer side already
     accepted wrappers (`with_linalg_array`/`linalg_value_to_array_value` route
     through `linalg_array_wrapper_value`), so no consumer fix was needed. Zero
     blast radius: full suite 3842/3842, AoT green, clippy/fmt clean, allowlist 5
     unchanged. Not benched (linalg is off every `vm_array_benchmark` path).
     Characterization fixture `linalg/decomposition_wrapper_producers_6807.jl`
     (23 asserts, Julia 1.12 parity) reuses each decomposition output downstream
     (indexing/size/matmul/equality). Still on the carrier afterwards:
     `type_ops/deep_copy.rs` (recursive, drives `copy`/`deepcopy`),
     `formatting.rs` (display/FFI boundary), the `Mark{BitVector,BitArray}` /
     `Reshape` constructors (need an override/`shared_parent`-preserving copy-free
     ctor), and the hot `exec/binary_both.rs` / `exec/array_index*.rs` paths
     (cache/dispatch-order sensitive — deferred to a careful late batch).
   - **Ratchet hygiene (done, PR #6855)**: dropped the two comment-only files
     (`formatting.rs`, `plotting/mod.rs`) from `NATIVE_ARRAY_ALLOWLIST` — their
     only `Value::NativeArray` match was a prose/doc comment, not a real variant
     use (both read arrays via the centralized `native_array_value_ref` helper).
     Reworded the comments to drop the bare token. Ratchet **5 → 3** (the
     remaining entries are the variant definition + enum arm `value_enum.rs`, the
     converter helpers `array_value/mod.rs`, and the carrier unit tests
     `frame.rs`).
   - **Slice 10 (done, PR #6857)**: flipped the F64-mode HOF return producers
     (`vm/hof_exec/dispatch.rs::handle_hof_return`) — the mapreduce/broadcast F64
     result arrays, the `findall` Int64 index arrays, and the broadcast / map /
     filter-in-place `dest` buffers — onto the wrapper. Added the returning-from-
     `ArrayRef` companion `push_array_ref_as_wrapper` (moves the inner
     `ArrayValue` out of a uniquely-owned ref). All six are freshly-built final
     outputs pushed after `clear_broadcast_state()`, never aliased or re-read.
     Full suite 3842/3842, AoT 3782/3782.
   - **Slice 11 (done, PR #6859)**: flipped the Slice-9-deferred hot
     `exec/binary_both.rs` dynamic binary-arithmetic result producers — the six
     scalar·array / matmul-fallback `self.stack.push(array_value(result))` sites —
     via `push_array_value_as_wrapper`. The dispatch-order-sensitivity concern
     was cleared by a full `cargo nextest run --release` (3842/3842) plus AoT
     (3782/3782). `try_matrix_diagonal_mul`'s two return sites stay on the carrier
     (a `&Vm`-immutable free function with no `struct_heap` access).
   - **Slice 12 (done, PR #6861)**: flipped the reflection `subtypes`
     (`builtins_types.rs`) and `@eval` `vect`-literal (`builtins_macro/eval.rs`)
     producers via a new returning companion `array_value_to_wrapper(&mut self,
     ArrayValue) -> Result<Value>` (`push_array_value_as_wrapper` refactored to
     share it). Removed the single-purpose `any_vector_array_value` native-carrier
     helper.
   - **Slice 13 (done, PR #6862)**: flipped the `Diagonal`-matmul
     (`exec/binary_both.rs::try_matrix_diagonal_mul`, signature `&Vm`→`&mut Vm`;
     callers already pass `self`) and native-array-*input* deep-copy
     (`type_ops/deep_copy.rs`) producers, retiring `binary_both`'s last
     native-carrier helper `array_value`.
   - **Slice 14 (done, this PR)**: flipped the dynamic broadcast-arithmetic
     fallbacks (`dynamic_ops::dynamic_add`/`dynamic_sub`/`dynamic_mul`/`dynamic_div`,
     signature `&self`→`&mut self`; all ~24 call sites already dispatch from a
     `&mut self` context) via `array_value_to_wrapper`, retiring
     `dynamic_array_value`. This completes every **readily-flippable** producer.
     Still on the carrier afterwards — all #6807-coupled / not cleanly flippable in
     isolation:
       - the host-return boundary `normalize_host_return_value` — intentional FFI
         re-materialization that **also resolves `StructRef` array elements into
         inline `Struct`s** for the heap-less host, and is load-bearing for the few
         integration/ffi tests asserting a native `run()` return (an outward-facing
         FFI-contract change);
       - the **compile-time** literal builder (`compile/utils.rs`) — runs with no VM
         / `struct_heap`, so it cannot build a heap-backed wrapper at compile time;
       - `formatting.rs` (display/FFI boundary), the `Mark{BitVector,BitArray}` /
         `Reshape` constructors (override / `shared_parent` storage), the
         consumer-entangled `iteration::extract_matrix_row/column` (static helpers
         that also *read* a native matrix), and the aliasing-sensitive
         `container.get_args` (`expr.args` must stay mutation-shared for `push!`);
       - and the structural #6807 blocker — the ~130 `native_array_value_ref`
         **consumer** borrow-sites, which (per the `splat::expand_*_with_heap`
         pattern) keep a native arm alongside a wrapper arm and are removed together
         when the variant is deleted.
3. **#6807** — once the allowlist is empty, remove the `Value::NativeArray`
   variant, the `ArrayValue` carrier, `native_array_compat.rs`,
   `array_wrapper.rs`, and the converter helpers, and flip the audit's
   `Value::NativeArray` policy to plain zero-match. Then close #6723.

## #6807 — variant removal: empirical blocker & plan (2026-06-18)

### Current surface

The `Value::NativeArray` carrier is down to a very small footprint:

- `scripts/check_value_array_allowlist.sh` ratchet is at **3 files**.
- The literal `Value::NativeArray` token appears in only **7 lines / 4 files**:
  the variant definition + one enum-method arm (`value/value_enum.rs`), the
  three converter helpers (`value/array_value/mod.rs`), and two carrier unit
  tests (`vm/frame.rs`).

The real surface is the helper call sites, not the variant text:

| helper | role | call sites |
|---|---|---|
| `native_array_value_ref` | borrow inner `ArrayValue` (read) | ~187 |
| `native_array_ref_from_value` | materialize value → `ArrayRef` | ~46 |
| `native_array_ref_value` | `ArrayRef` → carrier value (produce) | ~24 |
| `native_array_value_from_array` | `ArrayValue` → carrier value (produce) | ~18 |
| `native_array_value_mut_ref` | (retired in Slice 4) | 0 |

The ~187 read borrows each keep a *native arm + wrapper arm* (per the
`splat::expand_*_with_heap` pattern); deleting the variant deletes the native
arm and is mechanical **provided each producer already yields a wrapper**.

### Empirical blocker

Flipping the value-level producer helpers (`native_array_value_from_array` /
`native_array_ref_value`) to emit the faithful `Array{T,N}` wrapper as an
**inline `Value::Struct`** (the only wrapper form constructible without
`&mut Vm` / `struct_heap`) fails the full suite with 3 deterministic
**mutation** regressions:

- `varargs/varargs_parametric_where` — `result = T[]; push!(result, v)` →
  `Cannot modify field of immutable struct`
- `metaprogramming/..._expr_args_mutation_6616` — `ex.args` mutation →
  `expected numeric value, got Expr`
- `type_inference/builtin_op_inference` — downstream `AssertionError`

Root cause: an inline `Value::Struct` wrapper has **value semantics** — its
`size` field lives in the value-owned `values` vec, so `push!` / resizing
`setindex!` cannot write the grown length back to the caller's binding. The
carrier's `Rc<RefCell<ArrayValue>>` **reference semantics are load-bearing**
for in-place mutation (matching upstream, where `jl_array_t` is a mutable heap
object). Same wall as #6627. The remaining carrier producers are exactly those
that lack a reference-semantic, VM-context-free way to build a wrapper.

### Viable paths (multi-PR campaign)

- **B1 — thread VM/`struct_heap` context** through the remaining producers so
  they allocate reference-semantic heap `StructRef` wrappers. Most runtime
  producers already have `&mut self`; the genuinely context-free sites
  (compile-time literal builder) emit an *instruction* run at exec time with a
  VM, so the heap allocation can move there (as `PushArrayValue` did in Slice 2).
- **B2 — value-level reference-semantic wrapper** — give the faithful wrapper a
  representation that resizes through the already-shared `MemoryRef`
  (`Rc<RefCell<MemoryValue>>`), so `push!`/`setindex!` propagate without
  `struct_heap`. This removes the carrier without threading VM context
  everywhere.

**Decision (2026-06-18): B1 (heap-StructRef everywhere) is the upstream-faithful
path; B2 is ruled out.** `push!(a::Array, item)` does `a._mem = mem;
a._size = (new_len,)` — *field reassignments on `a`* (pure-Julia
`base/array.jl`) — so `a` must be a reference-semantic heap `StructRef`,
matching upstream's mutable `jl_array_t`. Deriving the length from the shared
`Memory` (B2) would diverge from the authoritative `size` field. The wrapper
constructor's only Vm dependency is `&mut self.struct_heap` +
`get_array_type_id()` (`exec/array_basic.rs::array_value_to_wrapper`).

### Empirical live-injector map (2026-06-18, 2533-fixture sweep)

Instrumented the two converter helpers with `#[track_caller]` and ran the whole
fixture corpus through `sjulia`. Native carriers are in **active hot-path
circulation**, not legacy. Fresh-build **root injectors** (`from_array`):
`hof_exec/value_mode.rs:855/881/886` (HOF value-mode results — *load-bearing for
#5229*: converts nested wrapper elements to native so StructRefs don't leak
through nested indexing/printing) and `exec/array_index_slice.rs:473/501/550`
(slice results). Re-wrap propagation (`ref_value`): `exec/locals.rs:691`
(typed-slot `LoadSlotArray`, the largest by count), `value/container.rs:1565`
(`expr.args` — `ExprContainer` stores its args as a native `ArrayRef`),
`exec/array_index.rs`, `builtins_arrays.rs:153`, `vm/frame.rs:200`,
`exec/array_mutate.rs:54`, `builtins_strings.rs:496`. Legacy/test-only (never
fired on real programs): the iteration matrix row/col path (`EachCol`/`EachRow`
have pure-Julia `iterate`), `value_enum.rs:577`, `deep_copy.rs:213`,
`struct_instance.rs`, `reflection/primitives.rs`.

The variant deletes once **all root injectors** are flipped (then the re-wrap
loop is dead: `slot_array` is only populated from a native carrier, so
`LoadSlotArray`'s native arm goes dead), at which point a green full suite proves
the ~187 `native_array_value_ref` consumer wrapper-arms cover every case and they
collapse mechanically.

### Progress (2026-06-18)

- **compile/utils.rs injector removed** — `eval_literal_default`'s array arms are
  dead post-#6876 (array-literal kw defaults are body-evaluated → fresh runtime
  wrapper); removed `literal_array_value` + the `native_array_value_from_array` /
  `ArrayValue` imports. The compile-time, no-VM injector is gone. Full suite green.
- **slice producers flipped** (`exec/array_index_slice.rs`) — `a[range]` /
  `a[idxvec]` / `m[rows, cols]` / n-dim slice *results* now emit the `Array{T,N}`
  wrapper via `array_value_to_wrapper` (`arr` is a local owned `ArrayRef`, so the
  outstanding `arr_borrow` does not conflict with `&mut self`). The internal
  `range.collect()` temp at line ~396 (consumed by `load_selected_array_elements`,
  which reads native) stays on the carrier. Fixture
  `arrays/slice_producers_wrapper_6807.jl` (18 asserts, 1.12.6 parity, pins
  slice-is-a-fresh-mutable-wrapper). Full suite 3843/3843, AoT green.
- **typeinfo-prefix display fix (#6882, merged)** — `value_show_type`'s
  `Value::Struct` arm now recognizes array-wrapper structs and mirrors the
  native-array arm, so a `Vector` of array-wrappers prints bare for an implicit
  inner eltype. This was the prerequisite for the HOF value-mode flip.
- **HOF value-mode result builder flipped (#6807)** —
  `hof_exec/value_mode.rs::create_typed_array_from_values` now emits the
  `Array{T,N}` wrapper (`array_value_to_wrapper`) and **no longer materializes
  nested array-wrapper elements to a native carrier** (the line-855 #5229
  conversion is removed — the #6882 formatter handles wrapper elements, so nested
  `map` results stay wrapper-of-wrapper and still display/index correctly).
  Fixture `hof/value_mode_nested_wrapper_result_6807.jl` (11 asserts). Full suite
  3845/3845, AoT green. (The other `array_value` sites in `value_mode.rs` — empty
  results, the `wrap_array_result==false` paths, `FindAll` empty Int64 — are
  separate producers, not yet flipped.)
### Milestone (2026-06-18): plain array programs are 100% wrapper-backed

A second instrumented sweep after the HOF/slice/compile-time flips, plus targeted
probes, narrowed the surface to a single root:

- A **plain array program** (literals, `getindex`, `setindex!`, `push!`, slices,
  matrices, `sum`, …) now produces **zero** native carriers — fully wrapper-backed.
- The **FFI host-return boundary** is already migrated:
  `normalize_host_return_value` returns an *inline* `Array{T,N}` wrapper
  (`array_wrapper_value_from_array_value_inline`), resolving `StructRef` elements
  to inline `Struct`s for the heap-less host (#6864) — it is **not** a native
  producer.
- The residual `value_mode.rs` `array_value` sites (empty / `wrap_array_result==false`
  / `FindAll`) **do not fire** on the fixture corpus.

**The entire remaining native-carrier surface in real programs is `expr.args`**
(`value/container.rs`). `ExprContainer.args` is an `Rc<RefCell<ArrayValue>>` and
`get_args()` (the sole exposure point) returns a native carrier sharing that `Rc`
for mutation aliasing (`push!(ex.args, x)` must mutate the `Expr` persistently).
The internal `self.args.borrow()` reads (`nargs` / `args_snapshot` / index) expose
no carrier. Every other live `native_array_ref_value` site (`locals.rs:691`
typed-slot, `array_index.rs`, `builtins_arrays.rs`, `frame.rs`, `array_mutate.rs`,
`builtins_strings.rs`) is **propagation** of carriers that originate at `expr.args`
— they go dead automatically once it is converted.

**Why `expr.args` is the deep core:** `push!(ex.args, x)` runs pure-Julia
`push!(a::Array, item)` which reassigns `a._mem` / `a._size`, so the args must be a
reference-semantic **heap `StructRef`** that the `Expr` persists — a transient
wrapper from `get_args()` can't (its `size` field isn't shared back, and replacing
`_mem` doesn't grow a shared `Memory`). That requires allocating the args on
`struct_heap` at `Expr` construction, but `ExprValue::from_head` is a free fn
(~100 call sites across `builtins_macro/parse.rs` + `ir_conversion.rs`) with no
`struct_heap`.

### Architectural finding (2026-06-18): `expr.args` legitimately needs the carrier

A deeper obstacle makes converting `expr.args` to a heap `StructRef` **wrong**,
not merely hard: `struct_heap` is an **append-only `Vec<StructInstance>` with no
per-value GC** (cleared only wholesale at a REPL session boundary). `ex.args` is
created for *every* `Expr` node — macro expansion and quote evaluation build whole
trees of transient `Expr`s — so heap-`StructRef` args would permanently occupy a
`struct_heap` slot per transient Expr → an **unbounded heap leak** for
metaprogramming-heavy / long-running (iOS) sessions. The native carrier
`Value::NativeArray(Rc<RefCell<ArrayValue>>)` is **reference-counted and
auto-freed** on drop — the correct semantics for transient, mutable `Vector{Any}`
args.

So the practical migration goal — get the no-JIT runtime off the legacy native
array for all *general* code — **is achieved**: plain programs and the FFI
boundary produce zero native carriers. The remaining carrier use is confined to
`expr.args`, where it is arguably correct. Deleting the variant *entirely* would
require either (1) **accept & confine** — keep the Rc-backed carrier as the
dedicated `expr.args` representation (optionally renamed, e.g. `Value::ExprArgs`),
and change the #6807 acceptance criterion from "zero match" to "no native carrier
outside `expr.args`"; (2) add a **`struct_heap` GC** first (large, separate); or
(3) an in-place-growable `Memory` with derived length for `expr.args` only
(diverges from the authoritative `size` field, fragile). Recommendation: (1).

### Resolution (2026-06-18): option 1 — accept & confine

The maintainer chose **option 1**. Implemented:

- The variant `Value::NativeArray(ArrayRef)` is **renamed `Value::ExprArgs(ArrayRef)`**
  with a doc comment stating its confined `expr.args` role. The `native_array_*`
  converter helpers (the generic carrier accessors) keep their names.
- `scripts/check_value_array_allowlist.sh` Policy 2 is reframed from a
  *ratchet-to-zero* into a **permanent confinement allowlist** (`EXPR_ARGS_ALLOWLIST`,
  3 files: variant def + enum arm, the `native_array_*` converter hub, the carrier
  unit tests). A variant-text match in any other file is a new carrier site outside
  `expr.args` and fails the audit.
- The #6807 acceptance criterion is **"no native carrier outside `expr.args`"**
  (the 3-file confinement), not "zero match". `CODE_AUDITS.md` updated to match.

The practical migration goal is complete: every general array value is the
MemoryRef-backed pure-Julia `Array{T,N}` wrapper; the `Value::ExprArgs` carrier is
confined to the metaprogramming `expr.args` representation, where its auto-freed
`Rc` semantics are correct.

### Update (2026-07, #8918): confinement is now a type, not a grep

The `EXPR_ARGS_ALLOWLIST` grep ratchet described above was **retired to a type**.
`Value::ExprArgs`'s payload is now the private-field witness newtype
`ExprArgsCarrier` (`subset_julia_vm_bytecode/src/value/array_value/mod.rs`): the
carrier can only be constructed or destructured through the `native_array_*` hub
in that module (the sole code with access to the private field), so an off-hub
carrier site is a **compile error** instead of an audit failure. Only the
`Value::Array` deleted-variant zero-match remains in
`check_value_array_allowlist.sh`. This follows the `Resolved` newtype template
(#8642); see `docs/vm/CODE_AUDITS.md`.

## Active Work

- Keep new code free of literal `Value::Array` matches.
- Keep `Value::NativeArray` compatibility converter call sites out of public
  defaults; new public behavior should use `Memory{T}` or Pure Julia
  `Array{T,N}` dispatch.
- Keep compatibility converter usage explicit and local to runtime, FFI,
  formatting, cache, and host-boundary code.
- Use `vm_array_benchmark` for Array migration performance tracking. #6653
  accepted an index/mutation regression versus the old native route and recorded
  a HOF/broadcast improvement; future optimization should target typed Memory
  storage and intrinsic hot loops, not a public `NativeArray` rollback.
- Use `docs/vm/CODE_AUDITS.md` as the audit policy source.
- Use `docs/vm/MEMORY_PRIMITIVE.md` and `docs/vm/MEMORYREF.md` for the active
  Memory/MemoryRef status notes.

## Validation

Relevant gates for migration work:

```bash
bash scripts/check_value_array_allowlist.sh
cargo clippy --all-targets -- -D warnings
timeout 1800 cargo nextest run --release
```

The `Value::Array` audit is expected to pass on current `main`; a new match
should be treated as either a bug or an intentional compatibility boundary that
needs explicit audit-policy documentation.

## Upstream References

Study these upstream files under `./julia` before changing a migration phase:

- `julia/src/jltypes.c` for builtin `GenericMemory`, `GenericMemoryRef`,
  `Array`, `Memory`, and type-layout initialization.
- `julia/src/array.c` for low-level allocation, storage ownership, and array
  runtime representation.
- `julia/base/essentials.jl` for bootstrap `GenericMemory` / `MemoryRef`
  constructors, bounds, `getindex`, and `setindex!`.
- `julia/base/array.jl` for `wrap(Array, Memory, dims)`, `reshape`, `collect`,
  and concrete `Array` methods.
- `julia/base/abstractarray.jl`, `julia/base/indices.jl`, and
  `julia/base/multidimensional.jl` for generic indexing, `similar`, and
  dimensional semantics.
- `julia/base/subarray.jl` for view representation and parent/index storage.
