# ConcreteType Lattice Reference (Issue #3187)

*Last updated: 2026-06-10*

## Overview

`ConcreteType` is the core enum in `compile/lattice/types.rs` representing specific runtime Julia types in the type lattice. There are currently **49 variants** (see TYPE_SYSTEM.md for the full inventory).

The lattice hierarchy is:

```
Bottom → Const(value, ConcreteType) → Concrete(ConcreteType) → Union → Conditional → Top
```

| Level | Meaning | Example |
|-------|---------|---------|
| `Bottom` | Unreachable code | Dead branch after `error()` |
| `Const(value)` | Known constant | `Const(42)` — more specific than `Concrete(Int64)` |
| `Concrete(ct)` | Known type, unknown value | `Concrete(Int64)` |
| `Union(set)` | One of several types | `Union({Int64, Float64})` |
| `Conditional` | Flow-sensitive | Type depends on branch taken |
| `Top` | Unknown (≈ `Any`) | Fallback when inference fails |

## ConcreteType Variant Categories

| Category | Count | Variants |
|----------|-------|----------|
| Signed integers | 6 | `Int8`, `Int16`, `Int32`, `Int64`, `Int128`, `BigInt` |
| Unsigned integers | 5 | `UInt8`, `UInt16`, `UInt32`, `UInt64`, `UInt128` |
| Floating point | 4 | `Float16`, `Float32`, `Float64`, `BigFloat` |
| Boolean | 1 | `Bool` |
| Text | 2 | `String`, `Char` |
| Special | 3 | `Any`, `Nothing`, `Missing` |
| Abstract numeric | 3 | `Number`, `Integer`, `AbstractFloat` |
| Symbolic | 1 | `Symbol` |
| Composite | 9 | `Array{el}`, `Tuple{els}`, `TupleVararg{prefix, tail}` (Issue #3511), `NamedTuple{fields}`, `Range{el}`, `Dict{k,v}`, `Set{el}`, `Generator{el}`, `Pairs` |
| User-defined | 1 | `Struct{name, type_id}` |
| Callable | 3 | `Function{name}`, `Closure{name}`, `ComposedFunction` |
| Type system | 2 | `DataType{name}`, `Module{name}` |
| IO | 1 | `IO` |
| Metaprogramming | 4 | `Expr`, `QuoteNode`, `LineNumberNode`, `GlobalRef` |
| Pattern matching | 2 | `Regex`, `RegexMatch` |
| Type unions | 1 | `UnionOf(Vec<ConcreteType>)` |
| Enum | 1 | `Enum{name}` (Issue #2863) |

**Note (resolved — Issues #9009 / #9034):** `Memory{T}` is now a full
`ConcreteType` lattice variant (`Memory { element: Box<ConcreteType>, ndims:
Option<usize> }`), mirroring the `Array` variant and reusing its aliasing /
mutation join behavior. Issue #9034 offered two options — add lattice tracking
or formalize the limitation — and this implements the tracking option. Before
this fix, `ValueType::Memory` / `ValueType::MemoryOf(T)` widened to
`LatticeType::Top`, so direct user-code `Memory{T}` inference reported `Any`.
Method-table parameter annotations of the form `m::Memory{Int64}` now resolve to
`ValueType::MemoryOf(I64)` instead of `ValueType::Any`. Residual gap:
`Memory{T}(undef, n)` constructor calls and indexed-load *return* types still
widen to `Any` (shared with `Array{T}(undef, n)` — parametric built-in
constructors are not inferred).

## Exhaustiveness Pattern

`ConcreteType` match arms are **intentionally exhaustive** (no wildcard `_` catch-alls) in critical functions. This ensures the Rust compiler emits an error when a new variant is added but not handled:

```rust
// CORRECT: exhaustive — compiler catches missing variants
match ct {
    ConcreteType::Int8 => 1,
    ConcreteType::Int16 => 1,
    // ... all 49 variants listed ...
    ConcreteType::Enum { .. } => 1,
}

// WRONG: wildcard hides missing variants
match ct {
    ConcreteType::Int64 => 2,
    _ => 1,  // New variants silently get depth 1
}
```

When you `cargo build` after adding a variant, the compiler will report `non-exhaustive patterns` errors at every site that needs updating. This is the primary safety net.

## `limit_type_size()` — Comparison-Aware Widening (Issue #3507)

Located in: `compile/lattice/widening.rs`

`limit_type_size(t, compare_to, max_length, max_complexity) -> LatticeType` is the sjulia counterpart of Julia's `limit_type_size` in `julia/Compiler/src/typelimits.jl`. It is used to bound the structural size (union length and nesting depth) of an inferred type relative to a reference comparison type.

Key properties:

- When every member of `t` is already structurally present in `compare_to`, `t` is returned unchanged — already-known complexity does not count.
- Only genuinely-new growth is charged against the per-call `max_length` budget. This preserves canonical small unions (`Union{T, Nothing}`, `Union{T, Nothing, Missing}`) under loop accumulators.
- Tuples / arrays / dicts past `max_complexity` collapse: tail-vararg-shaped tuples (a run of identical trailing elements) become a single-element tuple, mirroring Julia's `Vararg` handling. Other composite types lose their inner element types.
- When the budget is exceeded, the existing `widen_union` strategy (Issue #3539) is used: all-integer → `Integer`, all-float → `AbstractFloat`, mixed numeric → `Number`, otherwise `Top`.

`LatticeType::join_limited(other, compare_to)` is the recommended entry point at inference call sites where a comparison type is naturally available (e.g., the previously-known type for an SSA value before a loop body re-execution). `MAX_UNION_LENGTH` and `MAX_UNION_COMPLEXITY` (in `compile/lattice/widening.rs`) supply the default bounds. The first call site migrated is the loop-return accumulator in `compile/abstract_interp/engine/mod.rs`; remaining sites still use plain `join` and will be migrated in follow-up PRs.

Method-call union splitting uses a separate budget from lattice widening. For
ordinary method matching, `compile/abstract_interp/engine/mod.rs` uses
`MAX_METHOD_UNION_SPLIT_VARIANTS = 4`, mirroring Julia's
`InferenceParams.max_union_splitting`; `_apply_iterate`-style enumeration and
full `max_methods` controls remain follow-up work (Issue #4287).

## `type_depth()` — Nesting Depth

Located in: `compile/lattice/ops.rs`

`type_depth()` returns an integer representing the nesting depth of a `ConcreteType`. It is used by lattice join/meet operations to determine type ordering and widening thresholds.

| Depth | Types |
|-------|-------|
| 0 | Simple types: `Int64`, `Float64`, `Bool`, `String`, `Nothing`, etc. |
| 1+ | Parametric types: depth increases with nesting (e.g., `Array{Array{Int64}}` has depth 2) |

Every `ConcreteType` variant **must** have an explicit arm in `type_depth()`. The function is intentionally exhaustive to catch new variants at compile time.

## `TupleVararg` — Variadic-Tail Tuples (Issue #3511)

`ConcreteType::TupleVararg { elements, tail }` represents Julia's
`Tuple{T1, ..., Tn, Vararg{Tail}}` shape. It is produced by inference when
a varargs-call site has more arguments than
`ConcreteType::TUPLE_VARARG_NORMALIZE_THRESHOLD` (currently 8) so that
inferred call-argtypes / cache keys stay bounded:

- Short calls keep a precise flat `Tuple { elements }`.
- Long calls collapse to `TupleVararg { elements: [first], tail: join(rest) }`.
  The tail is the homogeneous element type when all trailing values are the
  same, otherwise a small `UnionOf` (capped at `MAX_UNION_LENGTH`, falling
  back to `Any` when too heterogeneous).

Subtyping is element-wise / Vararg-aware:
- `Tuple{T1, ..., Tn} <: Tuple{P1, ..., Pk, Vararg{Q}}` when the prefix
  matches element-wise (`Ti <: Pi`) and every remaining `Ti <: Q`.
- `Tuple{P1, ..., Pk, Vararg{Q1}} <: Tuple{P1', ..., Pk', Vararg{Q2}}` when
  prefixes match and `Q1 <: Q2`.
- `TupleVararg` is **not** a subtype of any flat `Tuple{...}` because it
  has an unbounded length.

Codegen / VM still see the generic `ValueType::Tuple` for these shapes —
the variant is currently inference-only.

## Checklist: Adding a New ConcreteType Variant

When adding `ConcreteType::Foo`:

### Core lattice files

- [ ] `compile/lattice/types.rs` — Add the variant to the `ConcreteType` enum
- [ ] `compile/lattice/types.rs` — Update `test_all_concrete_type_variants_constructible` coverage test
- [ ] `compile/lattice/types.rs` — `to_type_name()` (add string representation)
- [ ] `compile/lattice/types.rs` — `from_type_name()` (add reverse parse if applicable)
- [ ] `compile/lattice/types.rs` — `is_numeric()`, `is_integer()`, `is_float()` (add if numeric type)
- [ ] `compile/lattice/ops.rs` — `type_depth()` exhaustive match (add depth for new variant)

### Bridge conversions

- [ ] `compile/bridge.rs` — `From<&LatticeType> for ValueType` (ConcreteType → ValueType mapping)
- [ ] `compile/bridge.rs` — `From<&ValueType> for LatticeType` (ValueType → ConcreteType reverse mapping)
- [ ] `compile/bridge.rs` — `convert_concrete_to_array_element()` (add to appropriate arm)
- [ ] `compile/bridge.rs` — `concrete_type_to_julia_type()` (add specific mapping or verify `_ => JuliaType::Any` is correct)

### Verification

- [ ] Run `cargo build 2>&1 | rg "non-exhaustive patterns"` to find any remaining match sites
- [ ] Run `timeout 1800 cargo nextest run --release --lib` to verify coverage tests pass
- [ ] Update ConcreteType count in `TYPE_SYSTEM.md` and this file

## Keep Wildcard Arms Minimal

`type_depth()` in `ops.rs` is intentionally exhaustive — it forces every variant to declare its nesting depth. Do NOT add `_ => 1` wildcards. The compile errors when a new variant is missing are the desired safety net.

The same principle applies to bridge conversion functions. Prefer explicit arms over wildcards to maintain compile-time safety when variants are added.

## `PartialStruct` — Constructor Field Facts (Issues #8544 / #8739)

`LatticeType::PartialStruct { struct_name, type_id, field_names, fields }`
mirrors upstream `Core.PartialStruct` (`julia/Compiler/src/typelattice.jl`):
an instance of the immutable struct `struct_name` whose per-field lattice
facts are more precise than the declared field types. Field facts are
positional (`fields[i]` belongs to `field_names[i]`), may themselves be
`PartialStruct` (nested constructor chains, Issue #4269), and widen to
`Concrete(Struct)` via `widen_partial_struct()` (upstream `widenconst`).

Since Issue #8739 the lattice value is the **only** carrier of these facts:

- Default-constructor call sites (`infer_default_struct_constructor`,
  including bindable parametric instantiations) return the `PartialStruct`
  directly.
- Inner-constructor `new(...)` / `new{T}(...)` bodies produce the fact through
  the regular `Expr::New` arm of `infer_expr` (`infer_new_expr`, upstream
  `abstract_eval_new`), and constructor call sites with an available ctor body
  fall through to the ordinary interprocedural analysis instead of stopping at
  the method table's declared struct return type.
- Facts cross every boundary through the existing machinery: argument
  binding, `CachedReturn` world/backedge validity (#5603), the #8553 precise
  backedge index, and `TypeEnv` variable bindings. The former
  `ConstructorPartial` side cache, its `analyzing_partial_structs` recursion
  guard (#7186 — now bounded structurally by the regular
  `analyzing_functions` cycle guard), and the `TypeEnv::partial_structs`
  side table are deleted.

### Decision record (Issue #8739)

- **Mutable structs — deferred, design fixed.** Upstream allows
  `PartialStruct` facts for `const` fields of mutable structs only: in
  `abstract_eval_new`, `ismutable && !isconst(rt, i)` forces field `i` to its
  declared type because a later `setfield!` may replace the value. sjulia's
  parser/runtime has no `const` struct-field support yet, so *no* mutable
  field can currently carry a fact and mutable constructors intentionally
  widen to `Concrete(Struct)` everywhere (`infer_new_expr`,
  `infer_default_struct_constructor`). When `const` fields land, the
  implementation slot is the `is_mutable` branches of those two functions:
  keep declared types for non-`const` fields, keep argument facts for `const`
  ones — no new lattice variant is needed.
- **MustAlias — rejected for now.** Upstream `MustAlias` wraps a slot's
  *field reference* so `isa`/`===` constraints on `x.f` re-narrow later reads
  of the same field. sjulia already covers the practically observed cases
  with the `TypeEnv` refinement table (`x.f` path refinements from guards,
  root-alias groups, field-path aliases — Issues #3504/#3520/#4844), which is
  the same slot-keyed idea without a lattice element. Promoting it into the
  lattice would add a slot-identity invariant (upstream asserts these never
  nest and strips them at every boundary via `widenslotwrapper`) with no
  known sjulia workload benefiting. Revisit only with a concrete fixture that
  the refinement table cannot express.
- **PartialOpaque — rejected.** It models `Core.OpaqueClosure` construction;
  sjulia has no opaque closures (closures compile to named functions with
  explicit captures), so the element has nothing to describe. Out of scope
  until opaque closures themselves become a feature.

## Related Documentation

- `TYPE_SYSTEM.md` — Full type system architecture and all type representation layers
- `BINARY_DISPATCH.md` — How ConcreteType feeds into binary operator dispatch
- `CHECKLISTS.md` — Implementation checklists for new types, literals, etc.
