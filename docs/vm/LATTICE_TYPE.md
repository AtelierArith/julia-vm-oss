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

**Note:** `Memory` is a `ValueType` variant only, NOT a `ConcreteType` variant. Memory values at the lattice level are not yet tracked.

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

## Related Documentation

- `TYPE_SYSTEM.md` — Full type system architecture and all type representation layers
- `BINARY_DISPATCH.md` — How ConcreteType feeds into binary operator dispatch
- `CHECKLISTS.md` — Implementation checklists for new types, literals, etc.
