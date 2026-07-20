# Generator Representation — Retire-or-Keep Decision (Issue #9200 S6)

This note records the S6 (final) slice of the generator desugar epic
(Issue #9200): the **measured decision** on the fate of sjulia's native
`MakeGenerator` / `GeneratorCallable` bytecode representation and the eager
bracket-comprehension "FilterMap" fast path. It follows the
**Performance Decision Protocol** (`CHECKLISTS.md`, Issue #9129) and mirrors the
#9198 S6 Complex fast-path measurement.

## TL;DR — KEEP (by measurement), epic #9200 CLOSES

The native representation and the eager comprehension fast path are **retained**.
The A/B measurement shows that routing generator consumption through the pure
`iterate` protocol either **regresses 5–21×** or **cannot run at all**:

- `collect` / `sum` over a generator: **~21× slower** via pure iterate.
- Filtered `collect`: **errors** (the collapsed `FilteredFunctionIndex` callable
  is not drivable by the synchronous iterate collector) — correctness-load-bearing.
- Empty generator: eltype diverges (`Vector{Int64}` → `Vector{Any}`) — correctness.
- Eager bracket comprehension: **2.5–4.4× faster** than any `Base.Generator`
  representation.

**No `Instr` was tombstoned; no `CACHE_VERSION` bump.**

## The upstream ideal and why we measured

Upstream Julia (`julia/base/generator.jl`, `julia/src/julia-syntax.scm`
`expand-generator`) drives generators **purely by the iterate protocol** — a
`Base.Generator` is just a struct with two `iterate` methods, and every consumer
(`collect`/`sum`/`first`/`count`/`Tuple`) works through `iterate` with zero
generator-specific knowledge. S1–S4 desugared sjulia's generator *syntax* to
those upstream shapes (`Generator`/`Filter`/`Flatten`/`product`), but the
compiler still collapses them onto the native `MakeGenerator`/`GeneratorCallable`
runtime representation, and eager bracket comprehensions compile to a dedicated
array-building loop. S5 (PR #9465) surveyed the native consumers and found every
one load-bearing, deferring the retire-or-keep decision to this slice.

S6's question: **can sjulia reach the upstream iterate-only ideal without a perf
regression?**

## Decision formula (fixed before measuring)

> Pick X = **5%** (per the Protocol's precedent example; retiring a shipping
> hot-path optimization is only justified if it is essentially perf-neutral —
> comprehensions/generators are pervasive).
>
> **If** routing the S5-identified load-bearing generator consumers
> (`collect`-generator fusion, filtered `length`, generator `getindex`) **and**
> the eager-comprehension FilterMap fast path through the pure `iterate`
> protocol keeps output **byte-identical** **and** regresses the
> `collect`/`sum`/comprehension VM-only microbenches by **≤5%** (median,
> interleaved A/B), **RETIRE** the native `MakeGenerator`/`GeneratorCallable`
> `Instr` family (append-only tombstone + `CACHE_VERSION` bump) and the eager
> FilterMap fast path — reaching the upstream iterate-only ideal as a net
> simplification. **Else KEEP** them, documented, and the epic closes on the
> desugar shapes (S1–S4) delivered + the native representation retained by
> measurement.
>
> **Byte-identical is a HARD gate.** Any consumer whose pure-`iterate` fallback
> is not byte-identical (different shape / eltype / error class) is load-bearing
> for **correctness** and forces KEEP regardless of the perf number.

## Methodology

- Measurement gate `vm/generator_fastpath_gate.rs` (`set_generator_fastpath_disabled`,
  an `AtomicBool` mirroring `complex_fastpath_gate.rs`). When set, the two
  `collect(::Base.Generator)` interception sites in `exec/call_dynamic.rs` (the
  pre-score-boundary and the native-collect sentinel) route to
  `collect_iterator_via_iterate_protocol` — the synchronous, per-element
  pure-`iterate` collector — instead of the `collect_generator` HOF fast path.
  Default off = shipping.
- Bench `benches/vm_generator_representation_9200_benchmark.rs`
  (`cargo bench -p subset_julia_vm --bench vm_generator_representation_9200_benchmark`).
  `Vm::run()`-only over a precompiled `CompiledProgram`; two interleaved arms per
  `collect` shape; numeric parity + a small-N eltype/shape probe recorded before
  timing. The eager bracket comprehension (a compile-time array-building loop —
  not runtime-gateable) is compared against the `collect(Generator(...))`
  representations that a retired comprehension would re-lower to.
- **Protocol step 2 self-check.** The bench asserts up front that the two arms
  are genuinely different execution paths (via the empty-generator collect eltype,
  which differs `Vector{Int64}` vs `Vector{Any}`). The first gate implementation
  targeted a rarely-hit post-scoring `collect` site and was a **no-op** — the
  assertion caught it (both arms `Vector{Int64}`), invalidating an early "perf-
  neutral" reading; the real hot path is the pre-score-boundary site.
- N = 20 000. Provisional numbers, local run, ambient load present (some spreads
  ≈ ±10%); the CI measurement is authoritative per NORTH_STAR NS-4.

## A/B results (VM-only, median, N = 20 000, fixed gate)

| Shape | Fast path (shipping) | Pure-iterate collector (arm B) | Result |
|---|---|---|---|
| `collect(x*x for x in 1:N)` | 26.0 ms | 550 ms | **~21× slower** (+2015%), byte-identical |
| `collect(x*x for x in 1:N if iseven)` | 35.5 ms | **errors** (`FilteredFunctionIndex` not iterate-drivable) | correctness-load-bearing |
| `collect(y+1 for y in (x*x for x in 1:N))` (nested) | 132 ms | 715 ms | **~5.4× slower** (+596%), byte-identical |
| `sum(x*x for x in 1:N)` | 26.4 ms | 555 ms | **~21× slower** (+1986%), byte-identical |
| `collect(x*x for x in 1:0)` (empty) | `Vector{Int64}` | `Vector{Any}` | **byte-identical FALSE** (eltype) |

`sum` is gate-sensitive because `sum(g::Generator)` is defined `sum(collect(g))`
(`base/array.jl`), so it flows through the same `collect` fast path — the pure
iterate route is not a separate fast reduce loop but the slow synchronous
collector.

Eager bracket comprehension vs `Base.Generator` representations (same result):

| Shape | Eager loop | Generator fast path | Generator pure-iterate |
|---|---|---|---|
| `[x*x for x in 1:N]` | **10.3 ms** | 26.1 ms (2.5×) | 564 ms (55×) |
| `[x*x for x in 1:N if iseven]` | **8.1 ms** | 40.4 ms (5.0×) | errors |

## Applying the formula → KEEP

1. **`collect` / `sum` generator fast path:** the only existing pure-`iterate`
   collect (`collect_iterator_via_iterate_protocol`, a per-element interpreter
   re-entry) is **5–21× slower** than the `collect_generator` HOF fast path, far
   beyond X = 5%. **KEEP (perf).**
2. **Filtered `collect`:** the collapsed `FilteredFunctionIndex` callable **cannot
   be driven** by the synchronous iterate collector — the pure-iterate arm raises
   `TypeError`. Not routable at all today. **KEEP (correctness).**
3. **Empty generator eltype:** the fast path recovers the inferred `Vector{Int64}`;
   the pure-iterate route typejoins an empty list to `Vector{Any}` — byte-identical
   gate fails. **KEEP (correctness).** (Filtered `length` #9320 and `getindex`-based
   generics — surveyed in S5 — are the same class: not byte-identically routable.)
4. **Eager comprehension FilterMap loop:** the dedicated array-building loop is
   **2.5× (simple) to 5.0× (filtered)** faster than the best `Base.Generator`
   representation. Retiring it (re-lowering `[f(x) for x in xs]` to
   `collect(Generator(f, xs))`) is a large hot-path regression. **KEEP (perf).**

**Decision: KEEP** the native `MakeGenerator`/`GeneratorCallable` representation
and the eager comprehension FilterMap fast path.

### Recorded insight (Protocol step 5)

The measured pure-iterate arm is the **synchronous re-entrant collector**
(`collect_iterator_via_iterate_protocol`), a correctness fallback, not a
perf-optimized iterate-collect. The native iterate *driver*
(`start_lazy_generator_iterate_call`, used by `for`-loops) is fast in isolation,
so an optimized iterate-based `collect` *could* in principle approach the fast
path — but no such collect exists, and building one that also handles the
collapsed filter callable, empty-eltype recovery, and product N-D shape is
substantial work equivalent to what the fast path already provides. **The
upstream iterate-only ideal is therefore reachable only by re-implementing (not
deleting) the fast path — future work, not a free simplification.** The slow
synchronous collector being 5–21× off the fast path is itself a candidate
optimization target (it is reused by other builtins), tracked as a follow-up.

## Epic #9200 summary (S1–S6)

| Slice | PR | Delivered |
|---|---|---|
| S1 | #9394 | `Base.Generator{I,F}` parametrized to the upstream mirror; generator `IteratorSize`/`size` parity (#9379). |
| S2 | #9402 | Simple `(f(x) for x in it)` desugars to `Base.Generator(func, iter)`. |
| S3 | #9406 | Filtered `(f(x) for x in it if p(x))` → `Generator(f, Iterators.Filter(p, it))` (#9127/#9271/#9320). |
| S4 | #9449 | Product / flatten forms → upstream `Generator`/`product`/`flatten` shapes (#9325). |
| S5 | #9465 | Block-as-value control-flow tail fix (Closes #9358); generator consumer survey → all load-bearing, deferred to S6. |
| S6 | *(this)* | Measured retire-or-keep decision: **KEEP** the native fast-path representation; epic closes. |

**Outcome:** generator *syntax* is desugared to the upstream
`Generator`/`Filter`/`Flatten`/`product` shapes (structural parity, S1–S4); the
native `MakeGenerator`/`GeneratorCallable` runtime representation and the eager
comprehension FilterMap fast path are **retained by measurement** (S5/S6) as a
perf/correctness fast path underneath those shapes.

## Umbrella closure (Issue #10263)

The post-#9200 compatibility gaps that motivated #10263 are now closed as one
acceptance surface. The native representation remains the measured KEEP decision
above; completeness no longer depends on treating only one callable or placement
shape as the representative case.

| Acceptance boundary | Durable result |
|---|---|
| Module top-level generator/comprehension (#10227) | Lifted helpers nested anywhere in an expression tree are collected. #10346 made both ordinary and module-body statement visitors exhaustive, so a new `Stmt` variant cannot compile until its helper-collection behavior is classified. |
| Filtered generator used as a nested base (#9405) | `GeneratorCallable` represents direct, filtered, tuple-splat, type-object, captured runtime, and filtered-runtime callables; the synchronous nested boundary drives the inner generator through `iterate` instead of projecting a direct callable field. |
| Flatten-over-generators result typing (#9438) | `IteratorEltype(::Flatten)` mirrors upstream `_flatteneltype`; unknown inner-generator eltype selects observed-value `grow_to!` widening, recovering concrete/promoted result vectors. |
| All-filtered empty semantics (#10138) | Empty eltype reuse is predicate-provenance aware. Transparent predicates may preserve inferred element type; arbitrary user-call predicates produce `Vector{Union{}}` and the upstream empty-reduction behavior. |
| Cross-product prevention (#10050/#9566) | `ITERATOR_TRAITS.md` defines the shared contract and the generated 152-cell trait/consumer matrix has a zero-row skiplist. |

The broader protocol-driven consumer-kernel work in #10463 remains useful future
architecture, but it is not a residual failure of these acceptance cases and does
not reverse the measured native-representation decision. A future kernel must
first match the matrix's value, shape, eltype, dispatch, and error behavior and
then satisfy the same performance protocol before replacing a native path.

## Differential Trait Matrix (Issue #9566)

The representation decision above explains why the native fast path remains, but
it does not by itself prevent regressions across the generator trait/consumer
surface. Issue #9566 adds that prevention layer:

- **Generator:** `scripts/gen_generator_trait_matrix_fixture.jl` evaluates a
  deterministic upstream-Julia matrix over `map`, `filter`, `flatten`,
  `product`, `zip`, and `enumerate` generator shapes; `IteratorSize`,
  `IteratorEltype`, `collect`, `sum`, `foldl`, `first`, `length`, and explicit
  `for` consumption; and typed array / range / `Vector{Any}`-mixed bases.
- **Oracle:** `subset_julia_vm/tests/fixtures/generator/generator_trait_matrix_9566.tsv`
  records every upstream cell result.
- **Fixture:** `subset_julia_vm/tests/fixtures/generator/generator_trait_matrix_9566.jl`
  asserts every non-skiplisted cell under both upstream Julia and sjulia.
- **Skiplist:** `docs/vm/GENERATOR_TRAIT_MATRIX_SKIPLIST.tsv` tracks remaining
  divergences by cell id and Issue number. Removing a fixed row and re-running
  the generator promotes that cell into the executable fixture.
- **Ratchet:** `scripts/check_generator_trait_matrix.sh` verifies generated files
  are current, all skiplist ids exist, and the skiplist does not grow past the
  current residual count without an intentional ratchet update.

```bash
julia --startup-file=no scripts/gen_generator_trait_matrix_fixture.jl
bash scripts/check_generator_trait_matrix.sh
```

## Reproduce

```bash
cargo bench -p subset_julia_vm --bench vm_generator_representation_9200_benchmark
```

## Type-Only Generator Selector: Nominal Identity Decision (Issue #10879)

`IterateDynamic`'s `IteratorSize`/`IteratorEltype` trait dispatch
(`dynamic_call_generator_trait_name`/`dynamic_call_generator_trait_result` in
`subset_julia_vm_vm/src/vm/exec/call_dynamic.rs`) is the "type-only generator
dispatch path" named in #10879's prevention checklist: it decides whether a
call resolves to the Base `IteratorSize`/`IteratorEltype` semantics for a
`Value::Generator` purely by matching the *candidate function's name*
(`strip_module_prefix(func.name) == "IteratorSize"`), gated by
`!dynamic_call_has_user_function_name(...)` so it only fires when no user
override of that trait function exists at all. It never compares a concrete
struct's `type_id` against a declared parameter's origin the way
`function_candidate_has_nominal_origin_conflict` (Issue #10295/#10879) does
for every other selector in this document's blast radius.

This is an intentional, safe omission rather than a coverage gap:

- `Value::Generator` is a VM-synthesized wrapper for `map`/`filter`/`Base.Generator`
  comprehensions (see "Representation Decision" above) — it is not a
  user-declarable struct. There is no `StructDefInfo` a user module could
  register under a colliding bare name, so there is no *actual* nominal
  identity to erase or misattribute: the `ValueType`/`JuliaType` projection at
  this call site has no independent struct origin to compare in the first
  place, unlike a Base-cached concrete struct signature (`Partition`, `Rational`,
  ...) whose bare name a same-named external struct could otherwise satisfy.
- The applicability question this path answers is "does any *user* method
  override this trait name", not "does this candidate's declared concrete
  parameter type still belong to the same owner as the actual argument" — the
  name-based `dynamic_call_has_user_function_name` fence already answers the
  only question that can arise for a synthesized, non-instantiable type.

Consequence: no origin-aware fencing was added to this path as part of
#10879's selector-parity consolidation. If a future change makes `Generator`
(or any other native iterator wrapper sentinel) nominally shadowable by a
user-defined struct of the same bare name, this decision must be revisited
alongside `runtime_core_family_fallback_matches` and
`origin_safe_iterate_candidates` (Issue #10879), which fence the *other*
`IterateDynamic` family-fallback resolvers precisely because those resolvers
do compare real struct candidates against real struct arguments.
