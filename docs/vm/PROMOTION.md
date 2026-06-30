# Type Promotion in SubsetJuliaVM

This document describes how Julia's type promotion system is implemented in SubsetJuliaVM,
including the promotion rule registry lifecycle and the cache serialization path.

## Overview

Julia's type promotion is a three-layer system:

1. `promote_rule(T, S)` — Basic rules defined per type pair (e.g., `promote_rule(Int64, Float64) = Float64`)
2. `promote_type(T, S)` — Tries `promote_rule` in both directions and returns the common type
3. `promote(x, y)` — Converts values to the common type via `convert`

SubsetJuliaVM implements this in `subset_julia_vm/src/compile/promotion.rs`.

Bundled integer rules in `subset_julia_vm/src/julia/base/promotion.jl` are
spelled as explicit concrete `promote_rule(::Type{A}, ::Type{B})` pairs for
signed/unsigned integer and BigInt combinations. This preserves upstream
concrete `promote_type` results and primitive signed/unsigned value-level
`promote` conversion while keeping the rules extractable into the compile-time
promotion registry (Issue #6487). BigInt/narrow integer value conversion is
covered by the VM `BigInt` constructor's all-integer-width support (Issue #6489).

## Architecture

### Two Sources of Promotion Rules

Promotion rules come from two sources, applied in order:

1. **Julia-defined rules** (primary): Rules from `subset_julia_vm/src/julia/base/promotion.jl`
   are extracted at Base compile time and stored in a thread-local registry.
2. **Rust fallback** (secondary): If a rule is not found in the registry, the Rust
   implementation provides a fallback based on type priority (Float64 > Float32 > Int64 > ...).

This ensures Julia code is the source of truth, and users can extend promotion by adding
Julia `promote_rule` methods.

### Promotion Rule Registry (Thread-Local)

```rust
// subset_julia_vm/src/compile/promotion.rs
thread_local! {
    static PROMOTION_RULE_REGISTRY: RefCell<HashMap<(String, String), String>>;
    static REGISTRY_INITIALIZED: RefCell<bool>;
}
```

Key API:
- `register_promotion_rule(t1, t2, result)` — Register a rule
- `promote_type(t1, t2)` — Look up common type (checks registry, then Rust fallback)
- `get_all_promotion_rules()` — Dump all rules for serialization
- `clear_registry()` — Reset (used in tests)
- `is_registry_initialized()` — Whether Base compilation has run

## Lifecycle

```
Base Compilation
      │
      ▼
extract_promotion_rules_from_ir(functions)    ← reads Function IR bodies
      │
      │  Two body patterns:
      │  • Stmt::Expr { expr: Var("Int64") }           → "Int64"
      │  • Stmt::Expr { expr: Builtin(TypeOf, [Str(name)]) } → name
      │  • TypeVar-parametric bodies → skipped (generic methods)
      │
      ▼
register_promotion_rule(t1, t2, ret)          ← populates thread-local registry
      │
      ▼
mark_registry_initialized()                   ← signals registry is ready
      │
      ├──── Normal path (no cache): done
      │
      └──── Precompile path (--precompile-base):
                │
                ▼
            serialize_base_cache()
                │  uses get_all_promotion_rules() to read registry
                │  stored as Vec<(String, String, String)> in SerializedBaseCache
                │
                ▼
            bincode::serialize → bytes → embedded in binary
                │
                ▼
            Next startup: load_embedded_cache()
                │  reads embedded.promotion_rules
                │
                ▼
            register_promotion_rule() for each rule
            mark_registry_initialized()
```

## Known Bug History

### Issue #3025 — Registry Always Empty (FIXED)

**Root cause**: The old `extract_promotion_rules()` read `promote_rule` return types from
`MethodTable::MethodEntry.return_type`, which is always `ValueType::Any` after type inference
(because Julia's type inference cannot track "which type object a function returns").

**Fix**: `extract_promotion_rules_from_ir()` reads directly from the Function IR body
expression instead, correctly extracting both primitive and struct return types.

**Why `return_type = Any`**: Julia's `promote_rule` returns a type value (e.g., bare `Int64`
or `Complex{Float64}`). The compiler has no way to represent "this function returns the
type Int64 as a value" in the `ValueType` enum — there is no `ValueType::TypeValue` variant.
The function returns `DataType` at runtime, but the type inference conservatively infers `Any`.

**Key lesson**: Never extract semantic information from `MethodEntry.return_type` for
functions that return type objects — it will always be `Any`. Read the IR body instead.

### Issue #2489 — show_methods Lost in Cache (FIXED, similar pattern)

A structurally identical bug: `show_methods` (another compile-time-built registry)
was not serialized into the Base cache, so it was lost on subsequent runs.
The fix was to pre-populate it from `precompiled_base.show_methods`.

## Invariants

After `compile_base_functions()` completes:
- `is_registry_initialized() == true`
- `get_registry_size() > 50` (Base defines ~168 concrete `promote_rule` methods)
- `promote_type("Int64", "Float64") == "Float64"` (Julia-defined rule, not Rust fallback)
- `promote_type("Rational{Int64}", "Int64") == "Rational{Int64}"` (Julia-defined struct rule)

After `deserialize_base_cache()` + replay:
- Same invariants hold (verified by `test_promotion_rules_survive_serialize_deserialize_roundtrip`)

## Promote-fallback termination & the call-depth guard (Issues #5966, #5969)

Numeric binary operators have a generic promote-based fallback, e.g.

```julia
==(x::Number, y::Number) = (px, py = promote(x, y); px == py)
```

**This fallback only terminates when `promote` widens both operands to a type that
has a more-specific method.** If a mixed-type pair (e.g. `Real == Complex`) has **no
specific method** and `promote` fails to widen it, `px == py` re-dispatches the same
`==(::Number, ::Number)` on the **unchanged** pair forever → an unbounded VM call
stack (`#5966`: ~30 GB/worker before SIGTERM).

Rules when adding/extending a numeric type:
- Mirror upstream's **mixed-type** methods, not just the same-type ones — e.g.
  `==(z::Complex{T}, x::Real) where {T<:Real}` *and* `==(x::Real, z::Complex{T})`
  (plus `!=`). Prefer the **parametric** `Complex{T} where {T<:Real}` form over a bare
  `::Complex` (a bare abstract annotation can be loosely matched to a non-Complex value
  under specialization). Verify no mixed pair reaches the promote fallback.
- Audit the full numeric tower for promote-fallback reachability:
  `Complex / Rational / Irrational × Real / Integer`.

**Defense-in-depth:** even a missed mixed pair no longer OOMs — the VM call-depth guard
(`Vm::MAX_CALL_DEPTH`, checked in `run()` / `run_until_frame_return()`, Issue #5969)
turns runaway recursion into a catchable `StackOverflowError` (~80 MB transient). It is a
backstop, not a substitute for the mixed methods.

## VM runtime: promote-then-same-type in binary_both.rs (Issue #6338)

`vm/exec/binary_both.rs` mirrors upstream's promotion design for the dynamic
binary-op path (`execute_binary_both`):

1. `same_type_fast_path` — the only place SAME-type numeric pairs are matched
   with `Value::` patterns; it owns the per-type intrinsic op tables
   (Int64, Float64, Float32, Float16) and delegates the hot Int64×Int64 /
   Float64×Float64 pairs to `fast_primitive_binary_both`.
2. `promote_numeric_pair` — converts a heterogeneous pair to its common type
   (returning a `PromotedPairPolicy` that pins the legacy unsupported-op error
   label), after which `same_type_fast_path` is re-applied. The pair table must
   stay consistent with `compile/promotion.rs` and the rules in this document.
3. Anything else falls through to the tagged fallback chain and ultimately the
   Pure Julia promote fallback.

Only pairs whose current behavior is exactly "promote, then same-type op" may
be folded into layer 2. Behavior-exception pairs (Bool result types, Float16×Int
result-narrowing, unsigned widths, Int128 mixes, BigInt/BigFloat, Char) stay on
explicit arms — see the recursion-trap section above for why a silently dropped
pair is dangerous. Full arm inventory: docs/vm/BINARY_DISPATCH.md
("Promote-then-same-type structure") + `scripts/check_binary_both_fallback_inventory.sh`.

## Related Files

| File | Role |
|------|------|
| `subset_julia_vm/src/compile/promotion.rs` | Registry, `promote_type`, `promote_rule` fallback |
| `subset_julia_vm/src/compile/cache.rs` | `extract_promotion_rules_from_ir`, `compile_base_functions` |
| `subset_julia_vm/src/compile/precompile.rs` | `serialize_base_cache`, `deserialize_base_cache` |
| `subset_julia_vm/src/compile/embedded_cache.rs` | `load_embedded_cache`, replay path |
| `subset_julia_vm/src/julia/base/promotion.jl` | Julia-defined `promote_rule` methods |

## Testing

```bash
# Run all promotion-related tests
timeout 1800 cargo nextest run --release --lib -E 'test(promot)'

# Key regression tests
# test_promotion_rules_populated_after_base_compilation (Issue #3025)
# test_promotion_rules_survive_serialize_deserialize_roundtrip (Issue #3028)
# test_promotion_rules_populated_on_second_compile_with_cache (Issue #3028)
```
