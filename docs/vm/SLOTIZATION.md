# Array Slotization in SubsetJuliaVM

This document explains the slotization optimization pass and its interaction with global
array mutation builtins. Understanding this is essential to avoid `UndefVarError` regressions
when adding new array mutation functions. (See Issue #3121, #3127.)

---

## What is Slotization?

The VM compiler applies a lightweight local-variable optimization called **slotization** to
each compiled function. It replaces named-load/store instructions with indexed slot accesses,
which are faster and avoid repeated string lookups at runtime.

The pass lives in `subset_julia_vm/src/vm/slot.rs` and consists of two phases:

### Phase 1 – `build_slot_info`

Scans all instructions in a function body and collects the names of variables that have any
`Store*` instruction (e.g. `StoreArray`, `StoreI64`, `StoreF64`, …). Each distinct name gets
a numbered slot index.

```
StoreArray("xs") → slot 0
StoreI64("n")    → slot 1
```

Function parameters and keyword parameters are always assigned the lowest slot indices first.

### Phase 2 – `slotize_code`

Rewrites every `Load*` / `Store*` instruction that refers to a slotized name:

```
LoadArray("xs")  → LoadSlot(0)
StoreArray("xs") → StoreSlot(0)
LoadI64("n")     → LoadSlot(1)
StoreI64("n")    → StoreSlot(1)
```

---

## The Global-Array Hazard

### Problem

When a function body contains `StoreArray("GLOBAL_ARR")`, `build_slot_info` allocates
slot 0 for `GLOBAL_ARR`. Then `slotize_code` rewrites every `LoadArray("GLOBAL_ARR")` to
`LoadSlot(0)`.

At runtime, slot 0 starts **uninitialized** — the function has not stored anything there yet.
The first `LoadSlot(0)` therefore raises an `UndefVarError`, even though `GLOBAL_ARR` exists
as a perfectly valid global variable.

### Root Cause

Arrays in SubsetJuliaVM are `Arc`-ref-counted. A mutation builtin (e.g. `push!`) receives the
array by reference, modifies it **in place**, and leaves the same `Arc` on the stack. No
store back to the variable is needed for the mutation to be visible — the `Arc` already points
to the updated array.

`StoreArray` is only needed for **local** arrays where a new `Arc` might have been created
(e.g. after growing the array beyond its capacity). For global arrays it serves no purpose and
actively breaks slotization.

### Fix

Before emitting `StoreArray(name)`, check whether `name` is a global variable:

```rust
let is_global = self.locals.get(name).is_none()
    && self.shared_ctx.global_types.contains_key(name);
```

- **Local array** (`!is_global`): emit `StoreArray(name) + LoadArray(name)` as usual.
- **Global array** (`is_global`): skip `StoreArray` entirely. The mutated array is already
  on the stack (push-type) or discard the array reference with `Pop` (pop-type, after `Swap`).

---

## Helper Methods in `builtin.rs`

To ensure all mutation builtins apply the correct pattern, two helper methods are provided on
`CoreCompiler`:

### `compile_store_and_reload_array(name: &str)`

For **push-type** builtins (`push!`, `pushfirst!`, `insert!`, `deleteat!`):

```
Stack before: [..., modified_arr]   ← arr is on top after the mutation instruction
Stack after:  [..., modified_arr]   ← same; local: stored+reloaded; global: no-op
```

Implementation:
```rust
pub(crate) fn compile_store_and_reload_array(&mut self, name: &str) {
    if !self.is_global_array(name) {
        self.emit(Instr::StoreArray(name.to_string()));
        self.emit(Instr::LoadArray(name.to_string()));
    }
}
```

### `compile_store_or_pop_global_array(name: &str)`

For **pop-type** builtins (`pop!`, `popfirst!`), called **after `Swap`** has placed the
modified array on top of the stack:

```
Stack before: [..., value, modified_arr]   ← after Swap
Stack after:  [..., value]                 ← local: stored; global: popped
```

Implementation:
```rust
pub(crate) fn compile_store_or_pop_global_array(&mut self, name: &str) {
    if self.is_global_array(name) {
        self.emit(Instr::Pop);
    } else {
        self.emit(Instr::StoreArray(name.to_string()));
    }
}
```

---

## Checklist for New Array Mutation Builtins

When adding a new builtin that mutates an array in-place, follow this pattern:

- [ ] Load the array with `LoadArray(name)`
- [ ] Compile arguments and emit the mutation instruction
- [ ] **For push-type** (returns modified array): call `compile_store_and_reload_array(name)`
- [ ] **For pop-type** (returns removed value): emit `Swap`, then call
      `compile_store_or_pop_global_array(name)`
- [ ] Add a fixture test under `tests/fixtures/global_arrays/` that calls the new builtin from
      a function on a `const` global array and verifies the result

### Example fixture skeleton

```julia
# tests/fixtures/global_arrays/test_new_mutant.jl
const GLOBAL_ARR = Float64[]

function add_via_function(v)
    new_mutant!(GLOBAL_ARR, v)
end

@testset "new_mutant! on global array" begin
    add_via_function(1.0)
    add_via_function(2.0)
    @test length(GLOBAL_ARR) == 2
    @test GLOBAL_ARR[1] == 1.0
end

true
```

---

## Reference

- `subset_julia_vm/src/vm/slot.rs` — slotization implementation
- `subset_julia_vm/src/compile/expr/builtin.rs` — mutation builtins + helper methods
- `subset_julia_vm/src/compile/stmt.rs` lines 1160–1228 — `IndexAssign` with same `is_global` pattern
- Issue #3121 — original bug report
- Issue #3127 — prevention / refactoring tracking issue
