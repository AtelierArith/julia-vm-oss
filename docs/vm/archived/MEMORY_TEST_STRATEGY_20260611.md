# Memory Primitive Migration — Test Strategy

> **Archive note (2026-06-11):** This preserves the older phase-based Memory
> migration test plan. The active Memory status notes are
> `docs/vm/MEMORY_PRIMITIVE.md`, `docs/vm/MEMORYREF.md`, and
> `docs/vm/ARRAY_MEMORY_MIGRATION.md`; current audit policy lives in
> `docs/vm/CODE_AUDITS.md`.

This document defines the testing strategy for the Memory{T} primitive migration.
It covers the existing test inventory, Memory-specific test gaps, and a
phase-by-phase regression test plan.

Related historical material: Issue #2766 (stack overflow fix),
`docs/vm/archived/MEMORY_PRIMITIVE_ARCHITECTURE_20260611.md`.

## 1. Current Test Inventory

| Test Binary            | Count | Description                            |
|------------------------|------:|----------------------------------------|
| `fixture_tests`        | 1240  | Julia fixture tests (.jl files)        |
| `integration_tests`    | 638   | Rust integration tests                 |
| `lib` (unit tests)     | 796   | Rust unit tests in src/                |
| `dispatch_tests`       | 33    | Multiple dispatch tests                |
| **Total**              | **2707** |                                     |

### Key Categories for Memory Migration

These fixture test categories exercise Array/collection paths that will be
affected when the underlying storage transitions from `ArrayData` to `Memory{T}`:

| Category       | Tests | Impact Level | Reason                                      |
|----------------|------:|:------------:|---------------------------------------------|
| `array`        | 89    | High         | Core array ops: push!, pop!, indexing, etc.  |
| `broadcast`    | 50    | High         | Broadcasting allocates result arrays         |
| `collections`  | 35    | High         | Dict/Set use arrays internally               |
| `hof`          | 23    | High         | map/filter/reduce create new arrays          |
| `iteration`    | 46    | Medium       | Iterates over arrays                         |
| `linalg`       | 37    | Medium       | Matrix ops depend on array storage           |
| `sort`         | 11    | Medium       | In-place sorting modifies array storage      |
| `reduce`       | 11    | Medium       | Reductions traverse arrays                   |
| `subarray`     | 5     | Medium       | Views into arrays                            |
| `getindex`     | 5     | Medium       | Indexing operations                          |
| `dict`         | 13    | Medium       | Dict internals use Memory-based hash table   |
| `complex`      | 28    | Low          | Interleaved complex arrays                   |
| `strings`      | 67    | Low          | String arrays only                           |
| `memory`       | 4     | Direct       | Memory{T} API tests                          |

## 2. Memory-Specific Test Gaps

The existing `memory::` category has only 3 tests. The following gaps must be
filled before the migration can proceed:

### 2.1 Type Safety Tests (Priority: High)

- [ ] `Memory{Int64}` preserves element types through read/write cycles
- [ ] `Memory{Float64}` stores and retrieves IEEE 754 values exactly
- [ ] `Memory{Bool}` stores true/false correctly (no integer aliasing)
- [ ] `Memory{String}` stores reference types correctly
- [ ] `Memory{ComplexF64}` stores complex numbers (when interleaved storage is used)
- [ ] Type mismatch: storing Float64 into Memory{Int64} — should this convert or error?
- [ ] `typeof(Memory{Int64}(3))` returns the correct type

### 2.2 Resize / Capacity Tests (Priority: High)

- [ ] `memoryref_resize!` grows buffer and preserves existing elements
- [ ] `memoryref_resize!` shrinks buffer and discards tail elements
- [ ] Resize from 0 to N and from N to 0
- [ ] Resize to same size (no-op semantics)
- [ ] Large resize (1 → 1_000_000 elements)

### 2.3 Boundary Condition Tests (Priority: Medium)

- [ ] Empty Memory (length 0): length, size, iterate
- [ ] Single-element Memory
- [ ] Large Memory (10_000+ elements) — no stack overflow or excessive allocation
- [ ] Negative index access → BoundsError
- [ ] Zero index access → BoundsError (Julia is 1-indexed)
- [ ] Index beyond length → BoundsError

### 2.4 Copy/Similar Semantics (Priority: Medium)

- [ ] `copy(m)` creates independent shallow copy
- [ ] `similar(m)` creates uninitialized Memory of same size
- [ ] `similar(m, n)` creates uninitialized Memory of different size
- [ ] Mutation of copy does not affect original
- [ ] Copy of empty Memory

### 2.5 Array↔Memory Interop Tests (Priority: High — Migration-Critical)

These tests verify that the Array→Memory storage transition preserves semantics:

- [ ] Array backed by Memory: `push!`, `pop!`, `append!` still work
- [ ] Array indexing through Memory storage matches current behavior
- [ ] `collect(itr)` creates array with Memory backing
- [ ] Broadcast results stored in Memory-backed arrays
- [ ] `similar(array)` returns Memory-backed array
- [ ] Dict key/value storage through Memory

## 3. Phase-by-Phase Regression Test Plan

### Phase 1: Value::Memory Rust Type (Current)

**Goal**: Add `Value::Memory` variant without breaking existing tests.

**Test commands**:
```bash
# Memory-specific tests
timeout 300 cargo test --test fixture_tests memory::

# Unit tests for new Memory types
timeout 300 cargo test --lib memory
timeout 300 cargo test --lib value::memory

# Full regression (verify nothing breaks)
timeout 300 cargo test --test fixture_tests
timeout 300 cargo test --test integration_tests
```

**Pass criteria**: All existing tests pass. 4 memory fixture tests pass.

### Phase 2: Memory VM Instructions

**Goal**: Add NewMemory, MemoryGet, MemorySet, MemoryLength instructions.

**Test commands**:
```bash
# Memory operations
timeout 300 cargo test --test fixture_tests memory::

# Unit tests for new instructions
timeout 300 cargo test --lib vm::exec

# Categories that exercise getindex/setindex paths
timeout 300 cargo test --test fixture_tests array::
timeout 300 cargo test --test fixture_tests getindex::
timeout 300 cargo test --test fixture_tests collections::
```

**Pass criteria**: All existing tests pass + new Memory instruction tests.

### Phase 3: Connect Pure Julia genericmemory.jl to Rust Memory

**Goal**: `Memory{Int64}(n)` creates a Rust `Value::Memory` instead of a struct.

**Test commands**:
```bash
# Memory API tests (should use Rust Memory now)
timeout 300 cargo test --test fixture_tests memory::

# Array operations (arrays not yet migrated)
timeout 300 cargo test --test fixture_tests array::
timeout 300 cargo test --test fixture_tests broadcast::

# Full regression
timeout 300 cargo test
```

**Pass criteria**: Memory tests pass with Rust-backed Memory. All other tests unaffected.

### Phase 4: Compiler Array→Memory Code Generation

**Goal**: Compiler emits Memory-based instructions for array operations.

**Test commands**:
```bash
# Core array categories (highest impact)
timeout 300 cargo test --test fixture_tests array::
timeout 300 cargo test --test fixture_tests broadcast::
timeout 300 cargo test --test fixture_tests hof::
timeout 300 cargo test --test fixture_tests collections::

# Linear algebra (uses matrix operations)
timeout 300 cargo test --test fixture_tests linalg::

# Iteration (for-loops over arrays)
timeout 300 cargo test --test fixture_tests iteration::

# Sorting (in-place mutation)
timeout 300 cargo test --test fixture_tests sort::

# Integration tests (many use arrays)
timeout 300 cargo test --test integration_tests
```

**Pass criteria**: All 87 array + 45 broadcast + 23 HOF tests pass.

### Phase 5: Array Builtin Migration

**Goal**: Migrate built-in array functions to use Memory storage.

**Test commands**:
```bash
# Full fixture suite (every category potentially affected)
timeout 300 cargo test --test fixture_tests

# Dispatch tests (function dispatch may change)
timeout 300 cargo test --test dispatch_tests

# Unit tests (builtin implementation changes)
timeout 300 cargo test --lib
```

**Pass criteria**: All existing tests pass with Memory-backed arrays.

### Phase 6: Dict Migration to Memory

**Goal**: Dict uses Memory{UInt8}/Memory{K}/Memory{V} internally.

**Test commands**:
```bash
timeout 300 cargo test --test fixture_tests dict::
timeout 300 cargo test --test fixture_tests collections::
timeout 300 cargo test --test fixture_tests sets::
timeout 300 cargo test --test integration_tests
```

**Pass criteria**: All dict/collection tests pass.

### Phase 7: FFI/Consumer Updates

**Goal**: iOS, Web, Flutter consumers handle Memory values.

**Test commands**:
```bash
# Rust library tests
timeout 300 cargo test

# iOS build
cargo build --release --target aarch64-apple-ios
cargo build --release --target aarch64-apple-ios-sim

# WASM build
wasm-pack build --target web

# iOS app build
xcodebuild -project SubsetJuliaVMApp/SubsetJuliaVMApp.xcodeproj \
  -scheme SubsetJuliaVMApp -sdk iphonesimulator \
  -destination 'platform=iOS Simulator,name=iPad (A16)' build
```

**Pass criteria**: All Rust tests pass. All platform builds succeed.

### Phase 8: Final Cleanup (Value::Array Removal)

**Goal**: Remove `Value::Array` variant entirely.

**Test commands**:
```bash
# Full test suite — final regression
timeout 300 cargo test

# Verify no references to old ArrayData remain
grep -rn "Value::Array" subset_julia_vm/src/  # Should return 0 results
grep -rn "ArrayData" subset_julia_vm/src/      # Should return 0 results
```

**Pass criteria**: All tests pass. No `Value::Array` or `ArrayData` references in source.

## 4. Stack Overflow Fix (Issue #2766)

### Root Cause

Adding `genericmemory.jl` (159 lines, 7 functions) to the standard library
prelude pushed the recursive compilation stack usage past the default 8 MB
test thread limit. This affected ALL fixture tests, not just Memory tests.

### Fix Applied

`fixture_tests.rs`: `run_fixture_test()` now spawns each test on a dedicated
thread with `FIXTURE_TEST_STACK_SIZE = 16 MB`. This provides headroom for
current and future standard library growth.

### Verification

```bash
# These commands should all pass without RUST_MIN_STACK:
timeout 300 cargo test --test fixture_tests memory::
timeout 300 cargo test --test fixture_tests array::
timeout 300 cargo test --test fixture_tests  # full suite
```

## 5. Test Commands Quick Reference

```bash
# Memory category only (fastest feedback)
timeout 300 cargo test --test fixture_tests memory::

# High-impact categories (array/collection/broadcast)
timeout 300 cargo test --test fixture_tests array::
timeout 300 cargo test --test fixture_tests broadcast::
timeout 300 cargo test --test fixture_tests collections::
timeout 300 cargo test --test fixture_tests hof::

# Full regression (PR submission only)
timeout 300 cargo test

# Category listing
cargo test --test fixture_tests -- --list 2>/dev/null | \
  sed 's/::.*/::/;s/ .*//' | sort | uniq -c | sort -rn
```
