# TypeValue: `Type{T}` as a First-Class Compiler Value (Issue #10045)

**Status**: design document (RFC-adjacent). No code changes shipped with this
document; it records the current-state survey and a staged migration plan for
the `Type{T}`-representation gap identified by epic #10045 (`Type{T}` value
gap → ad-hoc promotion / eltype inference`).

## Summary

sjulia's abstract-interpretation lattice cannot represent "a compile-time
value that is itself a type" (`Type{Int64}`, the result of `typeof`, a
`where T` binding, …) as a first-class lattice element. Every subsystem that
needs to reason about such values today — `promote_rule`/`promote_type`,
array-literal `eltype`, generator `eltype` — reimplements its own ad-hoc,
string-keyed special case instead of sharing one representation. Milestone 62
(#9909) and its follow-up (#9914) patched the highest-impact symptoms without
closing the structural gap; #9914's own closing text says so explicitly:
*"The long-term structural gap remains: sjulia still cannot represent and
evaluate `Type{T}` values as first-class compile-time values."*

## Current State: How `Type{T}` Is Faked Today

### 1. The lattice has no type-object `Const` variant

`subset_julia_vm_types/src/runtime_types/lattice.rs` defines the abstract
interpretation lattice as `Top ⊒ Conditional ⊒ Union ⊒ Concrete ⊒ Const ⊒
Bottom` (module doc, lines 7–21). `LatticeType::Const(ConstValue)` (line 119)
is sjulia's analogue of upstream `Core.Const(val)` — but `ConstValue` (lines
44–57) is a closed enum of exactly six primitive payloads:

```rust
pub enum ConstValue {
    Int64(i64), Float64(f64), Bool(bool),
    String(String), Symbol(String), Nothing,
}
```

There is **no variant that can hold a type object** (a `ConcreteType`, a
`JuliaType`, or even a type name). A value statically known to *be* the type
`Int64` — e.g. the result of `typeof(1)`, or the constant argument to
`f(::Type{Int64})` — cannot be represented as `Const(...)`; it falls through
to a separate, structurally weaker carrier:

```rust
// subset_julia_vm_types/src/runtime_types/lattice.rs:317-319
DataType { name: String },
```

`ConcreteType::DataType { name: String }` is a **bare name string**, not a
recursive lattice element. It cannot nest (`Type{Vector{Int64}}` is just the
string `"Vector{Int64}"`), cannot carry `Union`/`Conditional`/parametric
structure, and every consumer that wants to do anything with it must
re-parse or re-dispatch on the string.

### 2. `promotion.rs`: a string-keyed registry plus a hand-written Rust fallback

`subset_julia_vm_types/src/promotion.rs` (535 lines) is the shared
`promote_type` implementation used by both the VM and AoT paths. Its own
module doc names the gap:

> "Julia code is the source of truth for promotion rules... Rust provides
> sensible defaults for bootstrapping and unknown types." (lines 19–22)

Two string-keyed layers do the real work:

- `PROMOTION_RULE_REGISTRY: RefCell<HashMap<(String, String), String>>`
  (line 36) — `promote_rule` methods extracted from
  `subset_julia_vm/src/julia/base/promotion.jl` at Base-compile time, keyed
  by type **name** pairs, not type values.
- `promote_rule_fallback(t1: &str, t2: &str) -> Option<String>` (line 155) —
  when the registry misses, a ~150-line hand-written Rust function that
  special-cases `Bool`, `BigInt`/`BigFloat`, `Rational{...}` (via
  `extract_rational_param`, string slicing on `"Rational{"`/`"}"`, line 135),
  `Complex{...}` (`extract_complex_param`, line 127, same string-slicing
  pattern), an explicit integer-width table (lines 254–264), and finally
  falls back to the shared `PrimitiveNumeric` taxonomy (line 291).

Every one of these branches exists because `promote_rule(T, S)` cannot be
*evaluated* — sjulia has no way to hand the Julia-level `promote_rule`
method a real `Type{T}` argument and let ordinary multiple dispatch pick the
method; it can only pattern-match on the type's **name string**.

### 3. `tfuncs/intrinsics.rs::tfunc_promote_type` — the #9914 landed shape

PR #9960 (closing #9914, cross-referenced via #9915) added a narrower tfunc
for `promote_type`, not a lattice-level `TypeValue`:

```rust
// subset_julia_vm_compile/src/compile/tfuncs/intrinsics.rs:308-334 (abridged)
pub fn tfunc_promote_type(args: &[LatticeType]) -> LatticeType {
    let mut names = Vec::with_capacity(args.len());
    for arg in args {
        let LatticeType::Concrete(ConcreteType::DataType { name }) = arg else {
            return tfunc_datatype_result(args); // generic DataType fallback
        };
        ...
        names.push(name.as_str());
    }
    let result = names.iter().skip(1)
        .fold((*first).to_string(), |acc, name| promote_type(&acc, name));
    LatticeType::Concrete(ConcreteType::DataType { name: result })
}
```

This keeps the *name* through `promote_type` calls when every argument is
already a `DataType{name}` (so nested calls like
`float(promote_type(Int64, Float32))` keep inferring a concrete type instead
of widening to `Any`), but it still round-trips through the same
string-based `promote_type` from `promotion.rs`, and it still bails to the
untyped `DataType { name: "" }` the moment any argument isn't already a
known type-name literal. It is a tfunc-level patch over the existing
carrier, not a new representation — confirmed by the regression test file
added alongside it,
`subset_julia_vm_compile/src/compile/abstract_interp/engine/tests/type_value_9914.rs`,
whose two tests (`promote_type_call_prefers_typevalue_tfunc_over_generic_body_9914`,
`promote_type_call_recovers_where_param_datatypes_9914`) both assert tfunc
*selection* behavior, not a new value representation.

### 4. `compile/expr/infer/array.rs` — one hand-written function per special family

Array-literal `eltype` inference (883 lines) is a second independent
consumer of the same gap. Rather than evaluating Julia's
`promote_typeof`/`eltype` logic once, it carries a parallel ladder of
special-cased helpers, each recognizing one type family **by name**:

- `is_irrational_type_name` / `is_irrational_struct` / `promotes_with_irrational_to_f64` (lines 8–43) — `Irrational{...}` singletons (Issue #9511).
- `promotion_type_name` (line 45) and `infer_promoted_numeric_element_type` (line 113) — calls back into `crate::compile::promotion::promote_type` (the same string function from §2) for ordinary numeric literals.
- `complex_or_real_promotion_name` (line 421) and `infer_mixed_complex_real_element_type` (line 457) — a dedicated Complex×Real reduction (Issue #6867), duplicating the `extract_complex_param` string-slicing from `promotion.rs`.
- `compute_union_display` (line 555) — yet another local `promote_numeric` closure (line 578) for `Union{Nothing, T}` display ordering.

Every new numeric family (a new `Irrational`-like singleton type, a new
`AbstractFloat` subtype, a user `Real` subtype with its own `promote_rule`)
requires a new hand-written branch here, because there is no general
"evaluate `Base.promote_typeof` over these element values" path to fall
back on.

### 5. Contrast: the *dispatch*-side `Type{T}` pattern already works better

Note this document's scope is deliberately narrower than "all `Type{T}`
handling in sjulia." `subset_julia_vm_types/src/inference_core/dispatch_resolver.rs`
already has a working, if separately-maintained, runtime `Type{T}`
pattern-matcher for **method dispatch** (`TypeObjectInnerFamily`, lines
2345–2367; the `Type{Pair{K,V}}` binding-extraction path fixed for #9987;
comments at lines 45–56 on "`Type{T}` singletons"). That matcher operates on
`JuliaType`/`CoreType`, a *different* type representation from the
`ConcreteType`/`LatticeType` lattice this document covers, and is out of
scope for the migration below — it is evidence that a real `Type{T}`
representation is tractable in this codebase, not a blocker.

## Bugs This Gap Caused

| Issue | Symptom | Root cause pattern |
|---|---|---|
| #9909 | Numeric/array-literal inference failures across the board | Named the gap; scoped the milestone-62 fallback consolidation |
| #9834 | `[0x1, 2]`, `[1f0, 2.0]`, `[1//2, 2]` fall to `Vector{Any}` outside a special-cased set | No general `promote_typeof` reduction for mixed numeric literals |
| #9511 | `[pi, 2.0]` stays `Vector{Any}` instead of promoting to `Vector{Float64}` | `Irrational` singletons need their own name-matched branch (§4) |
| #9464 | Struct names matching `/^[A-Z][0-9]*$/` (e.g. `S2`) collide with the TypeVar-name heuristic, breaking `Real`/`Number` dispatch | Type identity decided by matching bare *names*, so a struct name can alias a `where`-variable name |
| #9746 | Scoped `TypeVar`s regress to name-only matching | Same name-vs-identity confusion as #9464, in a different call path |
| #9955 (open) | `infer_return_type` widens `float(::Type{Float64})`'s result to `Any` | The `tfunc_promote_type` patch (§3) does not compose with every downstream consumer of `DataType{name}` |

Four of six are closed; #9955 remains open specifically because the #9914
patch is a point fix, not a structural one — a new caller shape can still
observe the `DataType{name: ""}` widening.

## Target Design: Upstream's `Type{T}` / `Const` Model

Verified against `julia/` (this repo's upstream reference checkout):

- **`Type{T}` is a real type, not a special case.** `isType(@nospecialize t) =
  isa(t, DataType) && t.name === _TYPE_NAME`
  (`julia/base/runtime_internals.jl:924`). `Type{Int64}` is the singleton
  type whose only instance is the type object `Int64`; ordinary Julia values
  of type `Type{Int64}` are dispatched, stored, and passed exactly like any
  other value — no separate "type value" machinery exists at the value
  level.
- **Inference tracks a known type object the same way it tracks any other
  known constant: `Core.Const`.** `Core.Const` is documented as *"the type
  representing a constant value"* whose single field `val` is unrestricted
  (`julia/base/coreir.jl:5-12`, `Compiler/src/typelattice.jl:9` imports it
  from `Core`). When inference proves an expression's value is exactly the
  type object `Int64`, the result is `Const(Int64)` — the *same* lattice
  element used for `Const(42)` or `Const(:sym)`, with no separate
  `DataType{name}`-shaped carrier. `PartialStruct` (`coreir.jl:15-37`) is the
  sibling extended-info element for partially-known structs; sjulia already
  mirrors `PartialStruct` faithfully (`LatticeType::PartialStruct`,
  `lattice.rs:167-177`, citing `julia/Compiler/src/typelattice.jl`
  explicitly) — `Const` is the one sibling sjulia's lattice does not fully
  mirror for type objects.
- **`promote_rule`/`promote_type` are ordinary multiple-dispatch Julia
  methods over `Type{T}` arguments**, resolved by the same method tables and
  `Const`-aware inference as any other generic function — not a
  string-keyed side table.

## Proposed Design: `TypeValue` as a `ConstValue` Variant

The structural fix is narrower than "redesign the lattice": extend
`ConstValue` (the closed enum blocking `Const` from carrying a type object,
§1) with one new variant, and route the existing name-string consumers
through it incrementally.

```rust
pub enum ConstValue {
    Int64(i64), Float64(f64), Bool(bool),
    String(String), Symbol(String), Nothing,
    // New:
    TypeValue(Box<ConcreteType>),  // a compile-time-known type object
}
```

- `LatticeType::Const(ConstValue::TypeValue(ct))` becomes the precise
  representation of "this expression's value is known to be the type
  object `ct`" — recursive (`ct` can itself be `Array { element: ... }`,
  `Struct { name, type_id }`, etc.), matching upstream's `Const(val::Type)`.
- `ConcreteType::DataType { name: String }` is **kept**, unchanged, as the
  *widened* representation — the analogue of upstream's imprecise `Type{T}
  where T` (a value known only to be "some type", not which one). This
  mirrors how upstream widens `Const(Int64)` to `Type{Int64}` and, further,
  to `DataType` when precision is lost; sjulia's existing `DataType{name}`
  already plays that "some type, possibly unknown" role and does not need
  to be removed.
- `promote_rule`/`promote_type`/array-literal `eltype` gain a fast path:
  when every operand lowers to `Const(TypeValue(...))`, evaluate the
  Julia-level `promote_rule`/`promote_typeof` method body through the
  existing abstract-interpretation engine (the same engine that already
  evaluates ordinary function bodies with `Const` arguments) instead of
  calling into `subset_julia_vm_types::promotion::promote_type`. The
  string-based Rust fallback stays as the **bootstrap-only** path (used
  before Base is compiled, or when an operand isn't reducible to
  `Const(TypeValue)`), exactly as `promote_rule_fallback` is already
  documented to be (`promotion.rs:97-101`).

## Staged Migration Plan

Each stage must keep `scripts/check_numeric_matrix_full_allowlist.sh`
green — that script currently asserts **zero** residual rows (Issue #9849
ratchet; see `docs/vm/NUMERIC_MATRIX_FULL_ALLOWLIST.tsv`), so any stage that
changes promotion output for even one numeric family must fix the
comparator, not add a new allowlist row.

1. **Stage 1 — add the lattice element, no behavior change.** Add
   `ConstValue::TypeValue`; teach `join`/`meet`/`widenconst` to fold it to
   `DataType{name}` immediately (i.e. every existing call site keeps its
   current output). Gate: full fixture suite unchanged bit-for-bit; new unit
   tests only assert the new variant's `join`/`widen` behavior in isolation.
2. **Stage 2 — route `tfunc_promote_type` through `Const(TypeValue)`
   evaluation**, keeping `tfunc_datatype_result` as the fallback for any
   argument that isn't reducible. Gate: `type_value_9914.rs` regressions
   plus a new fixture reproducing #9955 (`float(::Type{Float64})` no longer
   widens to `Any`); numeric matrix allowlist stays at zero.
3. **Stage 3 — route array-literal/generator `eltype`
   (`compile/expr/infer/array.rs`) through the same evaluation path**,
   retiring the per-family helpers (`is_irrational_type_name`,
   `complex_or_real_promotion_name`, …) one at a time as each is subsumed.
   Gate: #9511/#9834/#6867 fixtures stay green with the helper deleted, not
   just passing; `scripts/gen_numeric_matrix_fixture.jl` regenerated per the
   numeric-type-addition checklist in `AGENTS.md`.
4. **Stage 4 — shrink `promotion.rs`'s string fallback** to the documented
   bootstrap-only role (pre-Base-compile and genuinely non-reducible
   operands), removing the per-family branches
   (`extract_rational_param`/`extract_complex_param`/integer-width table)
   once every caller reaches them only through the `Const(TypeValue)` path.
   This stage is the one #9914 explicitly deferred; it should not be
   attempted before Stages 1–3 have working coverage, since
   `promote_rule_fallback` is still the only implementation for the
   bootstrap window.

Each stage is independently mergeable and independently revertible; no stage
requires the others to be "correct" in flight — Stage 2 falling back to
Stage 0 behavior on any miss is the safety property that makes incremental
landing possible.

## Non-Goals

- **Not a general `Core.Compiler`-style constant-propagation engine.** This
  proposal only extends the existing lattice with one more `Const` payload
  kind; it does not add generated-function-style "run arbitrary code at
  inference time" (upstream `@generated`) or full `UnionAll`
  instantiation/subtyping over runtime type parameters.
- **Not a rewrite of the dispatch-side `Type{T}` matcher.** The runtime
  `TypeObjectInnerFamily` pattern matching in `dispatch_resolver.rs` (§5)
  already works for method selection over `JuliaType`/`CoreType` and is out
  of scope; unifying the two type representations (`ConcreteType` used by
  the lattice vs. `JuliaType`/`CoreType` used by dispatch) is a larger,
  separate structural question this document does not take a position on.
- **Not a change to the wire/serialization format for cached `LatticeType`
  values** beyond what an added enum variant requires (bincode enum tags are
  declaration-order dependent per the existing `PartialStruct` comment,
  `lattice.rs:163-166` — the new variant must be appended, not inserted).
- **Not a fix for #9955 by itself** — #9955 is listed as the concrete
  regression test target for Stage 2, not something this document closes.

## Related Documentation

- `docs/vm/PROMOTION.md` — the promotion registry/fallback lifecycle this
  document's Stage 4 shrinks.
- `docs/vm/LATTICE_TYPE.md` — the `ConcreteType` lattice this document
  extends (`Const`/`PartialStruct` precedent).
- `docs/vm/TYPE_SYSTEM.md` — full type-representation architecture,
  including the `JuliaType`/`CoreType` dispatch-side representation
  referenced in §5.
- `docs/vm/DISPATCH_WORLD_AGE_RFC.md` — a sibling #10045 design document;
  independent of this one, but both touch how compile-time type/value facts
  flow into runtime dispatch decisions.
