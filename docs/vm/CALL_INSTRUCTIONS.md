# Call Instructions Guide

This document describes the call instruction handlers in the VM and the requirements for each.

## Call Instruction Inventory

| Instruction | File | Purpose |
|-------------|------|---------|
| `Call` | `exec/call.rs` | Basic function call (no kwargs at call site) |
| `CallWithKwargs` | `exec/call.rs` | Call with keyword arguments |
| `CallWithKwargsSplat` | `exec/call.rs` | Call with kwargs and splat expansion |
| `CallWithSplat` | `exec/call.rs` | Call with positional splat (no kwargs at call site) |
| `CallSpecialize` | `exec/call.rs` | Lazy AoT specialization |
| `CallIntrinsic` | `exec/call.rs` | Core intrinsic function call |
| `CallBuiltin` | `builtins_exec.rs` | Builtin function call |
| `CallDynamic` | `call_dynamic.rs` | Dynamic dispatch (single arg) |
| `CallDynamicOrBuiltin` | `call_dynamic.rs` | Unary dispatch with builtin fallback (length/size/ndims/eltype/similar, NegAny, floor/ceil/round/trunc) |
| `CallTypedDispatchOrBuiltin` | `call_dynamic_typed.rs` | Multi-arg typed dispatch with builtin fallback |
| `IterateDynamic` | `call_dynamic.rs` | Dynamic iterate() dispatch (1 or 2 args) |
| `CallDynamicBinary` | `call_dynamic_binary.rs` | Dynamic dispatch (binary ops, one Any operand) |
| `CallDynamicBinaryBoth` | `binary_both.rs` | Dynamic dispatch (both Any operands) + intrinsic fallback |
| `CallDynamicBinaryNoFallback` | `binary_no_fallback.rs` | Dynamic dispatch (no builtin fallback) |
| `CallTypedDispatch` | `call_dynamic_typed.rs` | Type{T} parametric dispatch |
| `CallTypeConstructor` | `call_dynamic_typed.rs` | T(x) type constructor call |
| `CallGlobalRef` | `call_function_variable.rs` | GlobalRef-based function call |
| `CallFunctionVariable` | `call_function_variable.rs` | Call function stored in variable |
| `CallFunctionVariableWithSplat` | `call_function_variable.rs` | Function variable call with splat args |

`call_dynamic_binary.rs` is the binary-dispatch entry point; it delegates
`CallDynamicBinaryBoth` to `binary_both.rs` and
`CallDynamicBinaryNoFallback` to `binary_no_fallback.rs`.

## Handler Requirements Checklist

When implementing or modifying a call instruction handler, ensure the following:

### 1. Positional Arguments

- [ ] Pop arguments from stack in correct order
- [ ] Bind arguments to parameter slots via `bind_value_to_slot()`
- [ ] Handle varargs: collect remaining args into `Value::Tuple`

### 2. Keyword Arguments

**CRITICAL**: Use the shared helper functions, not inline logic!

| Call Site | Helper Function | Used By |
|-----------|-----------------|---------|
| No kwargs provided | `bind_kwargs_defaults()` | `Call`, `CallWithSplat` |
| Kwargs provided | `bind_kwargs_with_map()` | `CallWithKwargs`, `CallWithKwargsSplat` |

These helpers handle:
- Required kwargs → `UndefKeywordError`
- kwargs varargs → empty `Pairs` or collected remaining kwargs
- Regular kwargs → provided value or default

### 3. Type Parameters

For functions with `where T` clauses:
- [ ] Extract type parameter bindings from arguments
- [ ] Handle `Val{N}` pattern for value-level type params
- [ ] Store bindings in frame for use in function body

### 4. Frame Setup

- [ ] Create frame with correct `local_slot_count`
- [ ] Push return IP to `return_ips` stack
- [ ] Push frame to `frames` stack
- [ ] Set `ip` to function entry point

## Shared Helper Functions

### `bind_kwargs_defaults()`

```rust
fn bind_kwargs_defaults(
    func: &FunctionInfo,
    frame: &mut Frame,
    struct_heap: &mut Vec<StructInstance>,
) -> Result<(), VmError>
```

**When to use**: When no kwargs are provided at the call site.

**Behavior**:
1. For each kwparam in function's kwparams:
   - If required → return `UndefKeywordError`
   - If `is_varargs` → bind empty `Pairs` (NOT `Nothing`!)
   - Otherwise → bind `kwparam.default`

### `bind_kwargs_with_map()`

```rust
fn bind_kwargs_with_map(
    func: &FunctionInfo,
    kwargs_map: &HashMap<String, Value>,
    frame: &mut Frame,
    struct_heap: &mut Vec<StructInstance>,
) -> Result<(), VmError>
```

**When to use**: When kwargs are provided at the call site.

**Behavior**:
1. For each kwparam in function's kwparams:
   - If `is_varargs` → collect remaining kwargs not matched to named kwparams into `Pairs`
   - If key in map → bind provided value
   - If required and not in map → return `UndefKeywordError`
   - Otherwise → bind `kwparam.default`

## Testing Checklist

When modifying call instruction handlers, add tests covering:

- [ ] Positional args only
- [ ] Kwargs only (named kwargs)
- [ ] Mixed positional + kwargs
- [ ] kwargs varargs with 0 kwargs passed
- [ ] kwargs varargs with multiple kwargs passed
- [ ] Positional splat (`f(args...)`)
- [ ] Positional splat with kwargs function (`f(x; kwargs...)` called via `f(args...)`)
- [ ] Kwargs splat (`f(; dict...)`)
- [ ] Required kwargs (error case)

### Fixture Test Locations

| Test | File |
|------|------|
| kwargs varargs empty | `tests/fixtures/kwargs/kwargs_varargs_empty.jl` |
| kwargs varargs with splat | `tests/fixtures/kwargs/kwargs_varargs_with_splat.jl` |
| kwargs multiple explicit | `tests/fixtures/kwargs/multiple_explicit.jl` |
| kwargs shorthand | `tests/fixtures/kwargs/shorthand.jl` |

## Code Review Checklist

When reviewing PRs that modify call instruction handlers:

- [ ] Are shared helper functions used instead of inline kwargs logic?
- [ ] Is `bind_kwargs_defaults()` used for handlers where no kwargs are provided?
- [ ] Is `bind_kwargs_with_map()` used for handlers where kwargs are provided?
- [ ] Are new test cases added for the modified behavior?
- [ ] Does the change apply consistently to ALL relevant handlers?

## Historical Issues

| Issue | Problem | Fix |
|-------|---------|-----|
| #2247 | `Call` bound kwargs varargs to `Nothing` instead of empty `Pairs` | PR #2268 |
| #2269 | `CallWithSplat` didn't bind kwargs at all | PR #2396 |
| #2397 | Duplicated kwargs logic across handlers | PR #2398 (shared helpers) |

## CallTypedDispatch (Issue #2587)

`CallTypedDispatch` handles Type{T} parametric dispatch (e.g., `promote_type(T, S)` where T,S are type parameters).

**Note**: The `sync_exec` dual execution path was **removed in PR #2796** (2026-02-12). There is now only a single execution path in `exec/call_dynamic_typed.rs`.

### Known Limitation

User-defined `promote_rule` methods called via `promote_type` may fail because
`promote_type` uses `CallTypedDispatch` with a frozen candidate list compiled
from Base methods. The runtime fallback in `call_dynamic_typed.rs` handles this,
but user-defined types may still hit edge cases.

## Type Parameter Binding (Issue #2468)

All call handlers extract type parameter bindings from arguments via `bind_type_params()`. This enables `where T` clause patterns to work with dynamic dispatch.

```rust
bind_type_params(&func, &args, &mut frame)
```

Handlers using this: `Call`, `CallWithKwargs`, `CallWithKwargsSplat`, `CallWithSplat`, `CallDynamic`, `CallDynamicOrBuiltin`, `IterateDynamic`, `CallDynamicBinary`, `CallDynamicBinaryBoth`, `CallDynamicBinaryNoFallback`, `CallGlobalRef`, `CallFunctionVariable`.

## Binary Method Dispatch Caching (Issue #2817)

Binary operator method dispatch results are cached by operand types in `binary_method_cache` (`vm/mod.rs`). This reduces repeated dispatch computation in hot loops with nary reduction patterns.

```rust
binary_method_cache: HashMap<BinaryDispatchKey, usize>
```

## Dict Parametric Mismatch Guard (Issue #2748)

All `CallDynamic*` handlers include a guard (`is_rust_dict_parametric_mismatch()`) that prevents Rust-backed `Value::Dict` from matching parametric `Dict{K,V}` methods. Pure Julia methods expecting `StructRef` would fail with "GetFieldByName: expected struct, got Dict" without this guard.

## Cache Alignment Invariant (Issue #2726)

When using precompiled Base cache, `function_infos` is initialized from the cache and user functions are appended at the end. This means cached bytecode contains `Call` instructions with indices that **must match** `function_infos` positions exactly.

### Invariant

For all Base function indices `i` (0 to `base_function_count - 1`):
```
all_functions[i].name == function_infos[i].name
```

A mismatch indicates that base function filtering has regressed — functions were reordered or incorrectly merged, causing `Call` instructions to invoke the wrong function at runtime.

### Enforcement

- **Debug assertion** in `compile/mod.rs`: After the function merging loop, a `#[cfg(debug_assertions)]` block verifies the invariant for all Base functions.
- **Exact signature matching**: Base function filtering (`is_same_function_signature`) must compare name, parameter types, and varargs status — not just the function name. This prevents false positives where a user function with the same name but different signature is incorrectly identified as a Base function duplicate.

### Prevention Test

`tests/fixtures/dispatch/test_user_base_coexist.jl` — Verifies that defining a user function with the same name as a Base function (e.g., `min`) does not break Base dispatch.

## Tfunc Metadata: Arity & Cost (Issue #3509)

Type-level dispatch during abstract interpretation is mediated by the
transfer-function registry in `subset_julia_vm/src/compile/tfuncs/`. Each
registered tfunc is now stored as a `TransferRule` carrying:

- `min_arity: usize` and `max_arity: Option<usize>` — argument-count bounds.
- `cost: u32` — relative inference / inlining cost (`COST_CHEAP = 1`,
  `COST_MEDIUM = 10`, `COST_EXPENSIVE = 100`, `DEFAULT_COST = 10`).

This mirrors Julia's `add_tfunc(f, minarg, maxarg, tfunc, cost)` in
`julia/Compiler/src/tfuncs.jl`.

When `infer_return_type` is called, the registry first checks `argc` against
the rule's arity range. Out-of-range calls return `LatticeType::Top` and emit
an `arity mismatch` diagnostic, instead of silently propagating to the tfunc.
Convenience constructors:

- `register_exact(name, arity, cost, fn)` — exactly `arity` arguments.
- `register_ranged(name, min, max, cost, fn)` — `min..=max` (or `min..` if
  `max` is `None`).
- `register(name, fn)` — legacy shim with `min=0, max=None, cost=DEFAULT_COST`.

Migrated entries today: arithmetic (`+`, `-`, `*`, `/`, comparisons, `!`),
`isa`, `typeof`, `zero`, `one`, `typemin`, `typemax`, unary maths
(`sqrt`, `abs`, `sin`, `cos`, `exp`, `log`), `min`/`max`, `print`/`println`.
Remaining tfuncs continue to use the legacy shim and accept any arity; they
will be migrated incrementally.

## Related Documentation

- `CLAUDE.md` - Shared Function Pattern
- `TYPE_SYSTEM.md` - Type parameter handling
- `LOWERING.md` - How kwargs are represented in IR
