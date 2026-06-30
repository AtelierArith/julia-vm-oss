# Memory{T} Rust Primitive Migration — Architecture Design

> **Archive note (2026-06-11):** This preserves the older Issue #2756
> architecture design. The active status note is
> `docs/vm/MEMORY_PRIMITIVE.md`; current array/memory migration tracking lives
> in `docs/vm/ARRAY_MEMORY_MIGRATION.md`.

**Issue**: [#2756](https://github.com/AtelierArith/ailujsoi/issues/2756)
**Milestone**: memory-primitive
**Last Updated**: 2026-02-13

---

## 1. Executive Summary

This document describes the architecture for migrating `Memory{T}` from a Pure Julia `mutable struct` to a Rust VM primitive (`Value::Memory`), and subsequently rebuilding `Array{T,N}` as a Pure Julia type wrapping `Memory{T}`. This aligns SubsetJuliaVM with Julia 1.11+ internals where `Memory` is the low-level buffer and `Array` is a high-level wrapper.

### Current State (as of 2026-02-13)

```
Value::Array(ArrayRef)   ← Rust primitive with ArrayData/ArrayValue/ArrayRef (still in use, 305 refs / 39 files)
Value::Memory(MemoryRef) ← NEW Rust primitive with type-segregated storage (88 refs / 27 files)
Dict{K,V}               ← Pure Julia struct using Memory-based hash table (PR #2788)
```

### Target State

```
Value::Memory(MemoryRef) ← NEW Rust primitive with type-segregated storage
Array{T,N}              ← Pure Julia struct wrapping Memory{T}
Vector{T} = Array{T,1}  ← Pure Julia alias
Matrix{T} = Array{T,2}  ← Pure Julia alias
```

### Why This Migration?

1. **Julia compatibility**: In Julia 1.11+, `Memory{T}` is the primitive and `Array` wraps it — our current design is inverted (Issue #2754)
2. **Type parameter preservation**: Pure Julia `Memory{T}` loses type parameters at runtime (`Memory{Int64}` → `Memory{Any}`)
3. **Performance**: Current Memory uses `Vector{Any}` internally, losing type-segregated storage benefits
4. **Dispatch limitations**: Built-in `setindex!`/`fill!` don't dispatch to Pure Julia methods for user structs — a Rust primitive resolves this
5. **Dict foundation**: Dict needs typed `Memory{UInt8}`/`Memory{K}`/`Memory{V}` for hash table internals (Issue #2763)

---

## 2. Architecture Overview

### 2.1 Julia Official Architecture (Reference)

In Julia 1.11+, the type hierarchy is:

```
GenericMemory{kind, T, addrspace}  ← C primitive (jl_genericmemory_t)
  Memory{T} = GenericMemory{:not_atomic, T, Core.CPU}

GenericMemoryRef{kind, T, addrspace}  ← C primitive (jl_genericmemoryref_t)
  MemoryRef{T} = GenericMemoryRef{:not_atomic, T, Core.CPU}

Array{T,N}  ← C struct wrapping MemoryRef + shape
  Vector{T} = Array{T,1}
  Matrix{T} = Array{T,2}
```

Key design: Memory handles **storage**, Array handles **shape and indexing**.

### 2.2 Current SubsetJuliaVM Architecture

```
Value::Array(ArrayRef = Rc<RefCell<ArrayValue>>)
  ArrayValue { data: ArrayData, shape: Vec<usize>, struct_type_id, element_type_override }
  ArrayData { F32(Vec<f32>), F64(Vec<f64>), I64(Vec<i64>), ..., Any(Vec<Value>) }

Memory{T}  ← Pure Julia mutable struct
  _data::Vector{Any}
  _length::Int64
```

**Problem**: Memory wraps Vector (Rust Array), but in Julia, Array wraps Memory. The dependency is inverted.

### 2.3 Proposed SubsetJuliaVM Architecture

```
Value::Memory(MemoryRef = Rc<RefCell<MemoryValue>>)
  MemoryValue { data: ArrayData, element_type: ArrayElementType }

# Pure Julia (in subset_julia_vm/src/julia/base/)
mutable struct Array{T, N}
    _mem::Memory{T}
    _size::Tuple
end

const Vector{T} = Array{T, 1}
const Matrix{T} = Array{T, 2}
```

**Key insight**: `ArrayData` (type-segregated storage) is reused as-is for `MemoryValue`. The only change is removing `shape` from the Rust primitive — shape moves to Pure Julia `Array`.

---

## 3. Type System Design

### 3.1 Value::Memory Variant

```rust
// In value_enum.rs
pub enum Value {
    // ... existing variants ...
    Memory(MemoryRef),  // NEW: Fixed-size typed buffer
    Array(ArrayRef),    // DEPRECATED: Kept during migration, removed in #2768
}
```

### 3.2 MemoryValue

```rust
// In vm/value/memory_value.rs (new file)

/// Fixed-size typed memory buffer (Julia's Memory{T})
/// This is the low-level storage primitive. Array{T,N} wraps this in Pure Julia.
#[derive(Debug, Clone)]
pub struct MemoryValue {
    /// Type-segregated storage (reuses existing ArrayData)
    pub data: ArrayData,
    /// Element type for this memory buffer
    pub element_type: ArrayElementType,
}

pub type MemoryRef = Rc<RefCell<MemoryValue>>;

pub fn new_memory_ref(mem: MemoryValue) -> MemoryRef {
    Rc::new(RefCell::new(mem))
}
```

**Design decisions**:
- **Reuses `ArrayData`**: No need to duplicate the 15-variant type-segregated enum
- **Reuses `ArrayElementType`**: Element type metadata is unchanged
- **No `shape`**: Memory is always 1D — shape belongs to Array (Pure Julia)
- **No `struct_type_id`**: Moved to ArrayElementType variants (`StructOf`, `StructInlineOf`)
- **No `element_type_override`**: The `element_type` field is always explicit

### 3.3 MemoryRef Decision

**Initial implementation: No MemoryRef type** (Issue #2765)

Julia's `MemoryRef{T}` provides offset-based access for zero-copy slicing. In SubsetJuliaVM:
- Slicing creates new arrays (copy semantics) in nearly all current code
- `reshape` is the only operation that could benefit from shared memory
- Adding MemoryRef introduces complexity (lifetime tracking, offset arithmetic) for minimal benefit

**Recommendation**: Omit MemoryRef initially. Revisit if `reshape` performance becomes critical.

### 3.4 Type System Integration

```rust
// In ValueType enum
pub enum ValueType {
    // ... existing variants ...
    Memory,                        // NEW
    MemoryOf(ArrayElementType),    // NEW: Memory with known element type
}

// In JuliaType enum
pub enum JuliaType {
    // ... existing variants ...
    Memory,                        // NEW: Memory{Any}
    MemoryOf(Box<JuliaType>),      // NEW: Memory{T}
}
```

---

## 4. VM Instruction Design

### 4.1 New Memory Instructions

```rust
// In instr.rs
pub enum Instr {
    // ... existing instructions ...

    // === Memory Operations ===

    /// Create a new Memory{T} of given size.
    /// Pop: argc dimension values from stack (currently always 1)
    /// Push: MemoryRef
    NewMemory(ArrayElementType, usize),

    /// Get element from Memory at index.
    /// Pop: index (Int64), Memory
    /// Push: element value
    MemoryGet,

    /// Set element in Memory at index.
    /// Pop: value, index (Int64), Memory
    /// Push: Memory (for chaining)
    MemorySet,

    /// Get length of Memory.
    /// Pop: Memory
    /// Push: Int64 (length)
    MemoryLength,
}
```

**Design rationale**:
- Only 4 instructions needed — Memory is deliberately minimal
- All higher-level operations (`push!`, `pop!`, `zeros`, `ones`, etc.) become Pure Julia using these primitives
- `MemoryGet`/`MemorySet` include bounds checking (Julia semantics)

### 4.2 Instruction Execution

```rust
// In vm/exec/memory.rs (new file)

fn exec_new_memory(vm: &mut Vm, elem_type: &ArrayElementType, argc: usize) -> Result<(), VmError> {
    // Pop dimension (always 1 for Memory — it's 1D)
    let size = vm.pop_i64()? as usize;
    let data = create_zeroed_array_data(elem_type, size);
    let mem = MemoryValue { data, element_type: elem_type.clone() };
    vm.push(Value::Memory(new_memory_ref(mem)));
    Ok(())
}

fn exec_memory_get(vm: &mut Vm) -> Result<(), VmError> {
    let index = vm.pop_i64()?;  // 1-indexed
    let mem_val = vm.pop()?;
    if let Value::Memory(mem_ref) = mem_val {
        let mem = mem_ref.borrow();
        if index < 1 || index as usize > mem.data.raw_len() {
            return Err(VmError::IndexOutOfBounds { ... });
        }
        let value = mem.data.get_value((index - 1) as usize)
            .ok_or(VmError::IndexOutOfBounds { ... })?;
        vm.push(value);
        Ok(())
    } else {
        Err(VmError::TypeError("expected Memory".into()))
    }
}
```

---

## 5. Migration Strategy

### 5.1 Phased Approach

The migration uses a **coexistence strategy**: `Value::Array` and `Value::Memory` exist simultaneously during the transition. This ensures all existing tests continue to pass at every phase.

```
Phase 1: Add Value::Memory (no existing code changes)           ✅ Done (PR #2771)
Phase 2: Connect Memory{T} constructor to Value::Memory         ✅ Done (PR #2774)
Phase 3: Connect builtins (memorynew, memoryref, etc.)          ✅ Done (PR #2776)
Phase 4: Value::Memory handling in medium-impact files           ✅ Done (PR #2777, #2780)
Phase 5: Update Value::Array references (305 sites, 39 files)   ✅ Audit floor reached (5 refs / 2 files; Issue #3908)
Phase 6: Migrate Dict to Memory-based hash table                ✅ Done (PR #2773)
Phase 7: Update FFI/external consumers                          🔧 Not started
Phase 8: Remove Value::Array variant (final cleanup)            🔧 Not started
```

### 5.2 Phase Details

#### Phase 1: Value::Memory Variant (#2757)

Add `Value::Memory(MemoryRef)` to the Value enum, `MemoryValue` struct, and `MemoryRef` type alias. No existing code is modified.

**Files**: `value_enum.rs`, `memory_value.rs` (new), `mod.rs`
**Tests**: Unit tests for MemoryValue creation/access
**Risk**: Low — additive only

#### Phase 2: Memory VM Instructions (#2758)

Add `NewMemory`, `MemoryGet`, `MemorySet`, `MemoryLength` instructions and their execution handlers.

**Files**: `instr.rs`, `exec/memory.rs` (new)
**Tests**: Integration tests for each instruction
**Risk**: Low — additive only

#### Phase 3: Connect genericmemory.jl (#2759)

Make `Memory{T}(n)` produce `Value::Memory` instead of `Value::Struct`. Update compiler to recognize Memory constructors. Simplify `genericmemory.jl`.

**Files**: `compile/expr/call/`, `julia/base/genericmemory.jl`
**Tests**: Existing memory fixture tests must pass
**Risk**: Medium — changes constructor dispatch

#### Phase 4: Pure Julia Array{T,N} (#2760)

Implement `Array{T,N}` as a `mutable struct` wrapping `Memory{T}`. Define `Vector{T}` and `Matrix{T}` aliases.

**Files**: `julia/base/array.jl`, `julia/base/abstractarray.jl`
**Tests**: New array-over-memory tests + existing array tests
**Risk**: High — largest single change

#### Phase 5: Array Builtin Migration (#2762)

Migrate Array builtins (`zeros`, `ones`, `push!`, `pop!`, etc.) from Rust to Pure Julia using Memory primitives.

**Files**: `builtins_arrays.rs`, `julia/base/array.jl`
**Tests**: All ~315 existing tests must pass
**Risk**: High — many builtins to migrate

#### Phase 6: Value::Array Reference Updates (#2764)

Update 305 `Value::Array` references across 39 files to work with Memory-backed arrays.

**Status (2026-05-25)**: ✅ Audit floor reached under Issue #3908 — the helper-consolidation work brought the count down to **5 references in 2 files** (the four shared destructure / construction helper bodies in `vm/value/array_value/mod.rs` plus the single multi-variant arm in `vm/value/value_enum.rs`'s `test_all_value_variants_constructed` exhaustive-coverage assertion). `scripts/check_value_array_allowlist.sh` enforces the floor; new `Value::Array` use is blocked outside the classified allowlist. Final retirement of the `Value::Array(ArrayRef)` enum variant is tracked separately as Phase 5 of `docs/vm/ARRAY_MEMORY_MIGRATION.md`.

**Priority order** (historical, by reference count at design time):
| Priority | Files | References |
|----------|-------|-----------|
| Phase 1 (High) | builtins_linalg.rs, type_ops.rs, array_index.rs, builtins_arrays.rs, array_basic.rs, builtins_exec.rs | 147 |
| Phase 2 (Medium) | call_dynamic_binary.rs, hof_exec.rs, builtins_sets.rs, dynamic_ops.rs, array_mutate.rs | 68 |
| Phase 3 (Low) | 28 remaining files | 81 |

**Risk**: High — widest blast radius

#### Phase 7: Dict Migration (#2763)

Migrate Dict from linear list (`Vec<(DictKey, Value)>`) to hash table using `Memory{UInt8}`/`Memory{K}`/`Memory{V}`.

**Files**: `container.rs`, `julia/base/dict.jl`
**Tests**: All dict tests must pass + new hash table tests
**Risk**: Medium — independent from Array migration

#### Phase 8: FFI & External Consumers (#2767)

Update FFI boundary code and external consumers (iOS, Web, Flutter).

**Affected files** (minimal impact):
- `ffi/format.rs` — `format_value()` for Memory display
- `repl.rs` — REPL array display
- `ffi/basic.rs` — Basic FFI value conversion
- `bin/sjulia.rs` — CLI display

**Risk**: Low — string conversion only

#### Phase 9: Final Cleanup (#2768)

Remove `Value::Array`, `ArrayValue`, `ArrayRef`, old Array instructions.

**Preconditions**: All ~315 tests pass, all FFI works, apps verified
**Risk**: Medium — large deletion, but well-verified

---

## 6. Dependency Graph

```
                    ┌──────────────┐
                    │  #2756       │
                    │  Design Doc  │
                    │  (this file) │
                    └──────┬───────┘
                           │
              ┌────────────┼────────────┐
              ▼            ▼            ▼
     ┌─────────────┐ ┌──────────┐ ┌──────────┐
     │ #2757       │ │ #2758    │ │ #2766    │
     │ Value::     │ │ Memory   │ │ Test     │
     │ Memory      │ │ VM Instr │ │ Strategy │
     └──────┬──────┘ └────┬─────┘ └──────────┘
            │             │
            └──────┬──────┘
                   ▼
          ┌─────────────────┐
          │ #2759            │
          │ Connect          │
          │ genericmemory.jl │
          └────────┬────────┘
                   │
          ┌────────┴────────┐
          ▼                 ▼
  ┌──────────────┐  ┌──────────────┐
  │ #2760        │  │ #2763        │
  │ Pure Julia   │  │ Dict →       │
  │ Array{T,N}   │  │ Memory-based │
  └──────┬───────┘  └──────────────┘
         │
  ┌──────┴───────┐
  ▼              ▼
┌──────────┐ ┌──────────┐
│ #2762    │ │ #2764    │
│ Builtin  │ │ 305-site │
│ Migration│ │ Update   │
└────┬─────┘ └────┬─────┘
     │            │
     └──────┬─────┘
            ▼
    ┌──────────────┐
    │ #2767        │
    │ FFI Update   │
    └──────┬───────┘
           ▼
    ┌──────────────┐
    │ #2768        │
    │ Final        │
    │ Cleanup      │
    └──────────────┘
```

### Issue Inventory

| # | Title | Dependencies | Risk |
|---|-------|-------------|------|
| #2756 | Architecture design document | None | — |
| #2757 | Value::Memory variant | #2756 | Low |
| #2758 | Memory VM instructions | #2756 | Low |
| #2759 | Connect genericmemory.jl to Rust primitive | #2757, #2758 | Medium |
| #2760 | Pure Julia Array{T,N} | #2757, #2758, #2759 | High |
| #2762 | Array builtin migration | #2760 | High |
| #2763 | Dict → Memory-based hash table | #2757, #2758, #2759 | Medium |
| #2764 | Value::Array 305-site update | #2757, #2760 | High |
| #2766 | Test strategy & existing test migration | #2756 | Low |
| #2767 | FFI/external consumer updates | #2760, #2764 | Low |
| #2768 | Final cleanup (remove Value::Array) | All above | Medium |

### Related Issues (Pre-existing)

| # | Title | Relevance |
|---|-------|-----------|
| #2746 | Memory{T} Pure Julia implementation | Completed (PR #2755) |
| #2753 | Vector{T} vs Vector{Any} for Memory storage | Resolved by Rust primitive |
| #2754 | Inverted Memory/Vector dependency | Root cause for this migration |

---

## 7. Impact Analysis

### 7.1 Code Impact

| Metric | Count (design-time, 2026-02-13) | Count (current, 2026-05-25) | Source |
|--------|----------------------------------|------------------------------|--------|
| `Value::Array` references | 305 | 5 (audit floor; Issue #3908) | 39 files → 2 files |
| `ArrayData`/`ArrayValue`/`ArrayRef` references | ~500 | unchanged (legacy storage still backs the `Value::Array(ArrayRef)` carrier) | 40 files |
| Array VM instructions | 21+ | unchanged | instr.rs |
| Array builtin functions | 35+ | builtins_arrays.rs, builtins_exec.rs |
| Existing tests | ~315 | fixtures + integration |
| Pure Julia array code | 2,928 lines | julia/base/array.jl + abstractarray.jl |

### 7.2 What Changes

| Component | Current | After Migration |
|-----------|---------|-----------------|
| Memory storage | `Vector{Any}` (Pure Julia) | `ArrayData` (Rust, type-segregated) |
| Memory type params | Lost at runtime | Preserved in `MemoryValue.element_type` |
| `setindex!` dispatch | Custom `mem_setindex!` | Standard `setindex!` via Rust primitive |
| Array shape | Rust `Vec<usize>` in ArrayValue | Pure Julia `Tuple` in Array struct |
| Array creation | Rust VM instructions | Pure Julia constructors + Memory |
| `push!`/`pop!` | Rust VM instructions | Pure Julia (Memory resize + copy) |
| `zeros`/`ones` | Rust builtins | Pure Julia (Memory + fill!) |
| Dict storage | `Vec<(DictKey, Value)>` | `Memory{UInt8}` + `Memory{K}` + `Memory{V}` |

### 7.3 What Stays the Same

- `ArrayData` enum (all 15 variants) — reused as `MemoryValue.data`
- `ArrayElementType` enum — reused as `MemoryValue.element_type`
- Column-major memory layout
- 1-indexed access semantics
- Type-segregated storage for performance
- Complex number interleaved storage pattern
- `Rc<RefCell<...>>` sharing pattern for mutability

### 7.4 FFI Impact

**Minimal** — external consumers only receive string representations:

| Consumer | Interface | Impact |
|----------|-----------|--------|
| iOS App (Swift) | String via C ABI | None — receives formatted text |
| Web App (WASM) | f64 or NaN | None — scalar results only |
| Flutter App (Dart) | String via FFI | None — receives formatted text |
| AoT Runtime | Own `TypedArray` | None — independent implementation |
| `format_value()` | `Value::Array` match | Update to handle `Value::Memory` |
| REPL display | `Value::Array` match | Update to handle `Value::Memory` |

---

## 8. Risk Assessment and Mitigation

### 8.1 Risk Matrix

| Risk | Likelihood | Impact | Mitigation |
|------|-----------|--------|------------|
| Regression in ~315 tests | High | High | Run full test suite after each phase; coexistence strategy |
| Performance regression (Pure Julia Array vs Rust) | Medium | Medium | Memory primitive keeps hot path in Rust; benchmark critical operations |
| Complex number storage incompatibility | Low | High | Interleaved storage pattern preserved in MemoryValue |
| Dict migration breaks existing Dict tests | Medium | Medium | Dict migration is independent; can be deferred |
| Stack overflow in Memory tests | Known | Medium | Fix pre-existing bug (Issue #2766) before migration |
| Type parameter loss in Pure Julia Array | Medium | Medium | Array struct stores type info; compiler recognizes Array constructors |

### 8.2 Rollback Strategy

Each phase is independently deployable:
- Phase 1-2: Additive only — no rollback needed
- Phase 3: Revert genericmemory.jl connection; Memory falls back to Pure Julia struct
- Phase 4-5: `Value::Array` coexistence means old code still works
- Phase 8: Only executed after all tests pass; pre-cleanup state is stable

### 8.3 Known Blockers

1. **Stack overflow in test framework** (discovered in PR #2755): Memory fixture tests cause stack overflow. Must be fixed before Phase 3 (Issue #2766).
2. **Type parameter limitations**: SubsetJuliaVM cannot use `where {T}` type parameters as runtime values. Rust primitive resolves this for Memory but Pure Julia Array may still face limitations.

---

## 9. Implementation Checklist

### Pre-Migration -- Completed

- [x] Fix stack overflow in Memory fixture tests (#2766) -- 16 MB thread stack size
- [x] Verify all existing tests pass on main branch
- [ ] Create benchmark baseline for array operations

### Phase 1: Value::Memory (#2757) -- Completed (PR #2771)

- [x] Add `MemoryValue` struct to `vm/value/memory_value.rs`
- [x] Add `Value::Memory(MemoryRef)` to `Value` enum
- [x] Add `ValueType::Memory` and `ValueType::MemoryOf`
- [x] Add `JuliaType::Memory` and `JuliaType::MemoryOf`
- [x] Implement `runtime_type()` for `Value::Memory`
- [x] Implement `value_type()` for `Value::Memory`
- [x] Add unit tests for MemoryValue CRUD operations

### Phase 2: Memory VM Instructions (#2758) -- Completed (PR #2771)

- [x] Add `NewMemory`, `NewMemoryDynamic`, `MemoryGet`, `MemorySet`, `MemoryLength` to `Instr` enum
- [x] Implement execution handlers in `vm/exec/memory.rs`
- [x] Add integration tests for each instruction
- [x] Verify bounds checking matches Julia semantics

### Phase 3: Connect genericmemory.jl (#2759) -- Completed (PR #2774, #2776)

- [x] Update compiler to recognize `Memory{T}(undef, n)` constructor -> `NewMemory`
- [x] Route `memorynew`, `memoryref`, `memoryrefget`, `memoryrefset!` to builtins
- [x] Verify `typeof(Memory{Int64}(5))` returns `Memory{Int64}`
- [x] All 4 existing Memory fixture tests pass

### Phase 4: Value::Memory handling -- Completed (PR #2777, #2780)

- [x] FFI layer Memory display
- [x] Medium-impact files handle Value::Memory
- [x] CLI display (sjulia.rs) Memory support

### Phase 5-8: See individual issue descriptions (not started)

---

## 10. Appendix: Current Array Instruction Inventory

### Array Creation Instructions
| Instruction | Stack Effect | Description |
|------------|-------------|-------------|
| `NewArray(cap)` | → arr | Create empty F64 array |
| `NewArrayTyped(elem, cap)` | → arr | Create typed array |
| `PushElem` | val arr → arr | Add element to array |
| `FinalizeArray(shape)` | arr → arr | Set shape |
| `FinalizeArrayTyped(shape)` | arr → arr | Set shape (typed) |
| `PushArrayValue(arr)` | → arr | Push literal array |
| `AllocUndefTyped(elem, argc)` | dims... → arr | Allocate uninitialized |

### Array Access Instructions
| Instruction | Stack Effect | Description |
|------------|-------------|-------------|
| `IndexLoad(n)` | idx... arr → val | Get element |
| `IndexLoadTyped(n)` | idx... arr → val | Get element (typed) |
| `IndexSlice(n)` | idx... arr → arr | Slice subarray |
| `IndexStore(n)` | val idx... arr → arr | Set element |
| `IndexStoreTyped(n)` | val idx... arr → arr | Set element (typed) |
| `LoadArray(name)` | → arr | Load from variable |
| `StoreArray(name)` | arr → | Store to variable |

### Array Mutation Instructions
| Instruction | Stack Effect | Description |
|------------|-------------|-------------|
| `ArrayPush` | val arr → arr | Append element |
| `ArrayPop` | arr → arr val | Remove last |
| `ArrayPushFirst` | val arr → arr | Prepend element |
| `ArrayPopFirst` | arr → arr val | Remove first |
| `ArrayInsert` | val idx arr → arr | Insert at index |
| `ArrayDeleteAt` | idx arr → arr | Delete at index |

### Random Array Instructions
| Instruction | Stack Effect | Description |
|------------|-------------|-------------|
| `RandArray(n)` | dims... → arr | Random F64 array |
| `RandIntArray(n)` | dims... → arr | Random I64 array |
| `RandnArray(n)` | dims... → arr | Randn F64 array |
| `RngRandArrayF64(n)` | dims... rng → arr rng | Seeded random F64 |
| `RngRandArrayI64(n)` | dims... rng → arr rng | Seeded random I64 |
| `RngRandnArrayF64(n)` | dims... rng → arr rng | Seeded randn F64 |

After migration, most of these instructions become unnecessary — replaced by Memory primitives + Pure Julia Array operations.
