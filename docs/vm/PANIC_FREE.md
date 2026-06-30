# Panic-Free VM Guidelines

This document describes the panic-prevention policies for the SubsetJuliaVM runtime. The VM must never panic during execution — all errors must be returned as `VmError` values.

## Why Panics Are Prohibited

SubsetJuliaVM targets iOS (App Store) and WebAssembly. Panics cause:

- **iOS**: Hard crash (SIGABRT), leading to App Store rejection
- **WASM**: `unreachable` trap, terminating the entire runtime
- **REPL**: Unexpected exit instead of graceful error message

All runtime errors must propagate as `Result<_, VmError>` so the caller can handle them.

## Clippy Lint Enforcement

### Crate-Level (Cargo.toml)

```toml
[lints.clippy]
unwrap_used = "warn"
expect_used = "warn"
```

This warns on any new `.unwrap()` or `.expect()` anywhere in the crate.

### Module-Level (VM Runtime)

All VM execution modules have strict deny attributes:

```rust
#![deny(clippy::unwrap_used)]
#![deny(clippy::expect_used)]
```

**Modules with deny attributes (43 total):**
- All 35 files under `src/vm/exec/`
- `src/vm/builtins_collections.rs`
- `src/vm/builtins_dicts.rs`
- `src/vm/builtins_exec.rs`
- `src/vm/builtins_numeric.rs`
- `src/vm/builtins_strings.rs`
- `src/vm/builtins_types.rs`
- `src/vm/builtins_types_conversion.rs`
- `src/vm/convert.rs`

### Exceptions

- **`build.rs`**: Uses `#![allow(clippy::unwrap_used)]` — build scripts should fail early
- **`#[cfg(test)]` modules**: Test code may use `.unwrap()` freely
- **`exec/mod.rs` SystemTime**: Single `#[allow(clippy::expect_used)]` for `SystemTime::now()`

## Approved Patterns

### Stack Operations

Use the `StackOps` trait instead of raw `.pop().expect()`:

```rust
// Bad: panics on empty stack
let val = self.stack.pop().expect("stack underflow");

// Good: returns VmError::StackUnderflow
let val = self.stack.pop_value()?;
let n = self.stack.pop_i64()?;
let s = self.stack.pop_str()?;
```

### Integer-to-usize Casts (Issue #3074, #2880)

Never cast `i64` directly to `usize` — negative values silently wrap around (e.g., `-1i64 as usize = 18446744073709551615`), causing OOM panics.

```rust
// Bad: silent wrap-around for negative values
let n = self.stack.pop_i64()? as usize;

// Good: returns VmError for negative values
let n = self.stack.pop_usize()?;
```

When working with array dimensions or indices from Julia values:

```rust
// Bad: direct cast from Value
let idx = match val { Value::I64(n) => *n as usize, _ => ... };

// Good: use pop_usize() or validate before casting
let idx = self.stack.pop_usize()?;
```

The `clippy::cast_sign_loss = "warn"` lint is enabled in `Cargo.toml` to catch new unsafe casts at CI time. If you must add `#[allow(clippy::cast_sign_loss)]`, include a `// SAFETY:` comment explaining why the cast is safe.

### Function Lookup

Use the shared helper methods:

```rust
// Bad: panics on invalid index
let func = &self.functions[index];

// Good: returns VmError via ?
let func = self.get_function_checked(index)?;

// Good: for try-catch contexts (clones to release borrow)
let func = self.get_function_cloned_or_raise(index)?;
```

### Option Unwrapping

Use `.ok_or_else()` to convert `Option` to `Result`:

```rust
// Bad: panics if None
let ch = s.chars().next().unwrap();

// Good: returns descriptive VmError
let ch = s.chars().next().ok_or_else(|| {
    VmError::TypeError("string is empty".to_string())
})?;
```

### External Library Results

Use `.map_err()` to convert external errors:

```rust
// Bad: panics on library error
let consts = astro_float::Consts::new().unwrap();

// Good: wraps in VmError::InternalError
let consts = astro_float::Consts::new().map_err(|e| {
    VmError::InternalError(format!("Failed to initialize: {}", e))
})?;
```

### Array Data Access

Use the `try_*` methods instead of panicking accessors:

```rust
// Bad: panics on type mismatch (methods removed)
let data = arr.data_f64();

// Good: returns VmError::TypeError
let data = arr.try_data_f64()?;
let data_mut = arr.try_data_f64_mut()?;
let as_f64 = arr.try_as_f64_vec()?;
```

### Guarded Unwraps

When a value is guaranteed by a prior check, use `match` instead of `.unwrap()`:

```rust
// Bad: relies on implicit guarantee
if !s.is_empty() {
    let first = s.chars().next().unwrap();
}

// Good: explicit safe pattern
let first = match s.chars().next() {
    Some(c) => c,
    None => return false,
};
```

### Safe Defaults

Use `.unwrap_or_default()` or `.unwrap_or()` only when the default is semantically correct:

```rust
// OK: parsing with default (BigInt(0) is acceptable fallback)
Value::BigInt(s.parse::<RustBigInt>().unwrap_or_default())

// OK: unwrap_or with explicit fallback
c.to_uppercase().next().unwrap_or(c)
```

### Bounding Host Recursion (Issue #5014)

Panic-free covers Rust panics, but a *native (Rust) stack overflow* is an
uncatchable process crash, not a `panic!`, so it must be prevented up front.
When a builtin drives the interpreter *synchronously* and can re-enter itself
on the host call stack, bound the nesting depth and surface
`VmError::StackOverflow` before the host stack is exhausted.

The `eval` builtin is the canonical case: `eval_dispatch_call` runs nested VM
calls via `run_until_frame_return`, which may re-enter the `eval` builtin, so a
program like `f() = eval(Meta.parse("f()"))` would otherwise grow the Rust
stack without bound and crash. The fix bounds it with a counter:

```rust
// Entry of the re-entrant builtin path:
self.enter_eval_dispatch()?; // -> Err(VmError::StackOverflow) past the bound
let result = /* drive the nested call */;
self.exit_eval_dispatch();   // ALWAYS decrement (success and error paths)
result
```

The bound (`Vm::MAX_EVAL_DISPATCH_DEPTH`) is generous enough for ordinary
metaprogramming but small relative to the native stack. Always pair `enter_*`
with an unconditional `exit_*` so the counter never leaks.

## Adding New Modules

When creating a new file under `src/vm/exec/` or `src/vm/builtins_*.rs`:

1. Add deny attributes at the top of the file (after doc comments):

```rust
//! Module description.

#![deny(clippy::unwrap_used)]
#![deny(clippy::expect_used)]

use ...;
```

2. Use `Result<_, VmError>` for all fallible operations
3. Use the patterns described above for error handling
4. Run `cargo clippy` to verify no unwrap/expect violations

## Try/Catch Catchability (Issue #2918, #3061)

Panic-free is **not** sufficient. VM instruction handlers must also make user-visible runtime errors *catchable* by Julia's `try/catch` mechanism.

### Two Error Propagation Paths

| Path | Mechanism | Catchable by `try/catch`? |
|------|-----------|--------------------------|
| Direct `return Err(VmError::...)` or `result?` | Bypasses `handle_error`, propagates to Rust caller | **No** |
| `self.raise(err)?` | Calls `handle_error`, routes to nearest catch block | **Yes** |
| `self.try_or_handle(result)?` | Calls `handle_error` on `Err`, returns `Ok(None)` if caught | **Yes** |

### When to Use Each Pattern

**`self.raise(err)?` + `return Ok(XxxResult::Continue)`** — user-visible runtime error:

```rust
// Index out of bounds, InexactError, DomainError, etc.
if idx < 1 || idx > len {
    self.raise(VmError::IndexOutOfBounds {
        indices: vec![idx],
        shape: vec![len],
    })?;
    return Ok(ArrayIndexResult::Continue);
}
```

**`self.try_or_handle(result)?`** — converting a `Result` (e.g., from a helper function):

```rust
let value = match self.try_or_handle(collection.get(index).cloned())? {
    Some(v) => v,
    None => return Ok(ArrayIndexResult::Continue),
};
```

**`return Err(VmError::...)`** — internal VM invariant violation (wrong type on stack = compiler bug):

```rust
// Only for "this should never happen" cases triggered by a compiler bug
return Err(VmError::InternalError(format!("Expected Int64, got {:?}", val)));
```

### VmError Variant Classification

Choosing the right `VmError` variant is as important as choosing the right propagation path.
Use `// INTERNAL:` or `// User-visible:` comments at each error site to make intent explicit.

| Variant | When to use | Propagation path |
|---------|-------------|-----------------|
| `VmError::InternalError` | Compiler-invariant violation — stack shape mismatch caused by a bug in the compiler (e.g., `NewArrayTyped` not emitted before `PushElemTyped`). User code cannot trigger this. | `return Err(...)` — bypass try/catch, surface to Rust caller |
| `VmError::TypeError` | Runtime type mismatch triggered by user code (e.g., calling a function with the wrong argument type). Julia's `try/catch` must be able to catch this. | `self.raise(err)?` + `return Ok(...::Continue)` |
| `VmError::IndexOutOfBounds` | Array/tuple index out of range — raised by user code. `BoundsError` in Julia. | `self.raise(err)?` + `return Ok(...::Continue)` |
| `VmError::UndefVarError` | Undefined variable reference at runtime. | `self.raise(err)?` + `return Ok(...::Continue)` |
| `VmError::StackUnderflow` | Stack underflow — always a compiler bug. | `return Err(...)` via `StackOps` |
| `VmError::MethodError` | No matching method found for call. | `self.raise(err)?` + `return Ok(...::Continue)` |

#### Quick Decision Flowchart

```
Could user Julia code trigger this error?
  YES → self.raise(VmError::TypeError/IndexOutOfBounds/...) and // User-visible: comment
  NO  → return Err(VmError::InternalError(...)) and // INTERNAL: comment
```

#### Examples

```rust
// INTERNAL: compiler always emits NewArrayTyped before PushElemTyped.
return Err(VmError::InternalError(
    "PushElemTyped: expected TypedArray on stack (compiler invariant)".to_string(),
));

// User-visible: indexing with wrong type — catchable by try/catch.
self.raise(VmError::TypeError(
    "MethodError: no getindex method for String with range index".to_string(),
))?;
return Ok(ArrayIndexResult::Continue);
```

### Borrow Conflict Pattern

When the error value is computed while `self` is borrowed (e.g., from `self.struct_heap`), extract the result in a block to release the borrow before calling `try_or_handle`:

```rust
// Build result while heap is borrowed, then release borrow before try_or_handle.
let result = {
    let s = self.struct_heap.get(idx);
    match s {
        Some(s) if in_range => Ok(s.values[i].clone()),
        _ => Err(VmError::IndexOutOfBounds { ... }),
    }
}; // borrow of self.struct_heap ends here
match self.try_or_handle(result)? {
    Some(v) => v,
    None => return Ok(TupleResult::Continue),
}
```

### Code Review Checklist

When adding or modifying a VM instruction handler:
- [ ] Does the handler use `self.raise(err)?` (not `return Err(...)`) for user-visible runtime errors?
- [ ] Are `Result`s from helper functions routed through `self.try_or_handle()` instead of `?`?
- [ ] Is `return Err(...)` reserved only for internal VM invariant violations?
- [ ] After `self.raise(err)?`, is `return Ok(XxxResult::Continue)` used to resume the dispatch loop?

## Regression Prevention Tests

The `panic_free_vm_tests.rs` test file ensures that `.unwrap()`, `.expect()`, and `panic!()` counts don't increase over time:

```rust
// These tests will fail if panic-inducing code is added to VM runtime
#[test] fn vm_unwrap_count_does_not_regress() { ... }
#[test] fn vm_expect_count_does_not_regress() { ... }
#[test] fn vm_panic_count_does_not_regress() { ... }
```

**Current baselines:**
- `.unwrap()`: 0 (all are in test code or doc comments)
- `.expect()`: 1 (SystemTime in exec/mod.rs — acceptable)
- `panic!()`: 0

If these tests fail after your changes:
1. Refactor to use the approved patterns above
2. If truly necessary, update the baseline with a comment explaining why

## Related PRs

| PR | Description |
|----|-------------|
| #2186 | Extract shared `get_function_checked()` method |
| #2188 | Remove `.unwrap()` from exec modules |
| #2190 | Remove `.expect()` from exec modules |
| #2192 | Remove `.unwrap()`/`.expect()` from non-exec VM modules |
| #2195 | Add crate-level Clippy lint config |
| #2197 | Remove panicking array accessor methods |
| #2366 | Add panic-free regression tests (Issue #2193) |
