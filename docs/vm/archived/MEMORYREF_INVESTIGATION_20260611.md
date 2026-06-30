# MemoryRef Investigation: Feasibility for SubsetJuliaVM

> **Archive note (2026-06-11):** This preserves the older Issue #2765
> feasibility investigation. The active status note is
> `docs/vm/MEMORYREF.md`; current array/memory migration tracking lives in
> `docs/vm/ARRAY_MEMORY_MIGRATION.md`.

> Issue #2765 — Investigate whether SubsetJuliaVM needs MemoryRef and when to implement it.

## 1. MemoryRef in Official Julia

### 1.1 Struct Definitions (julia/base/boot.jl)

In Julia (1.11+), the memory hierarchy is:

```julia
# Fixed-size raw storage buffer
struct GenericMemory{kind::Symbol, T, AS::AddrSpace}
    length::Int
    const data::Ptr{Cvoid}
    # hidden: elements or owner
end

# Pointer into GenericMemory at a specific offset
struct GenericMemoryRef{kind::Symbol, T, AS::AddrSpace}
    mem::GenericMemory{kind, T, AS}
    data::Ptr{Cvoid}
end

# Type aliases
const Memory{T}    = GenericMemory{:not_atomic, T, CPU}
const MemoryRef{T} = GenericMemoryRef{:not_atomic, T, CPU}

# Array holds a MemoryRef (not Memory directly) + dimensions
mutable struct Array{T,N} <: DenseArray{T,N}
    ref::MemoryRef{T}
    size::NTuple{N,Int}
end
```

### 1.2 Key Relationship

```
Array{T,N}
  ├── ref::MemoryRef{T}
  │     ├── mem::Memory{T}  (the actual data buffer)
  │     └── data::Ptr        (pointer to a position within mem)
  └── size::NTuple{N,Int}    (dimensions)
```

MemoryRef is essentially `(Memory, offset)` — a fat pointer that knows both the underlying buffer and where within it to start reading. The `memoryrefoffset(ref)` intrinsic returns the 1-based index of `ref` within `ref.mem`.

### 1.3 Constructor Functions (julia/base/boot.jl:620-632)

```julia
# Create a MemoryRef pointing to the start of a Memory
memoryref(mem::GenericMemory) = memoryrefnew(mem)

# Create a MemoryRef pointing to index i in a Memory
memoryref(mem::GenericMemory, i::Integer) = memoryrefnew(mem, Int(i), @_boundscheck)

# Advance an existing MemoryRef by i positions
memoryref(ref::GenericMemoryRef, i::Integer) = memoryrefnew(ref, Int(i), @_boundscheck)
```

## 2. Use Cases in Official Julia

### 2.1 Zero-Copy Reshape (julia/base/reshapedarray.jl:43-51)

`reshape(::Array)` creates a **new Array sharing the same MemoryRef** — no data is copied:

```julia
function reshape(a::Array{T,M}, dims::NTuple{N,Int}) where {T,N,M}
    len = Core.checked_dims(dims...)
    if len != length(a)
        throw_dmrsa(dims, length(a))
    end
    ref = a.ref  # reuse the same MemoryRef
    return $(Expr(:new, :(Array{T,N}), :ref, :dims))
end
```

This is the primary reason `Array` holds a `MemoryRef` instead of a `Memory` directly — multiple Arrays can share the same underlying Memory buffer through their MemoryRefs.

### 2.2 Efficient push!/popfirst! via Offset Management (julia/base/array.jl:1103-1155)

MemoryRef's offset enables O(1) `pushfirst!`/`popfirst!` when there is unused space at the beginning of the Memory:

```julia
function _growbeg!(a::Vector, delta::Integer)
    ref = a.ref
    len = length(a)
    offset = memoryrefoffset(ref)
    newlen = len + delta
    # If there's space before the current offset, just shift the ref backward
    if delta <= offset - 1
        setfield!(a, :ref, @inbounds memoryref(ref, 1 - delta))
        setfield!(a, :size, (newlen,))
    else
        # Need to reallocate
        @noinline _growbeg_internal!(a, delta, len)
        setfield!(a, :size, (newlen,))
    end
end
```

Similarly, `_growend!` checks if there's space after the end of the array within the Memory.

### 2.3 wrap() — Create Array from Memory/MemoryRef (julia/base/array.jl:3201-3248)

The `wrap(Array, mem, dims)` function creates an Array directly from a Memory or MemoryRef:

```julia
function wrap(::Type{Array}, m::MemoryRef{T}, dims::NTuple{N, Integer}) where {T, N}
    dims = convert(Dims, dims)
    ref = _wrap(m, dims)
    $(Expr(:new, :(Array{T, N}), :ref, :dims))
end
```

This enables creating multiple array views of the same Memory with different shapes.

### 2.4 Array Copy (julia/base/array.jl:383-386)

`copy(::Array)` allocates new Memory but uses MemoryRef for efficient copying:

```julia
ref = a.ref
newmem = typeof(ref.mem)(undef, length(a))
@inbounds unsafe_copyto!(memoryref(newmem), ref, length(a))
return $(Expr(:new, :(typeof(a)), :(memoryref(newmem)), :(a.size)))
```

### 2.5 Atomic Operations (julia/base/genericmemory.jl:350-424)

AtomicMemoryRef (= `GenericMemoryRef{:atomic, T, CPU}`) enables per-element atomic operations on AtomicMemory:

```julia
function getindex_atomic(mem::GenericMemory, order::Symbol, i::Int)
    memref = memoryref(mem, i)
    return memoryrefget(memref, order, @_boundscheck)
end
```

## 3. SubsetJuliaVM Current State

### 3.1 Current Array Implementation

SubsetJuliaVM's `ArrayData` (in `vm/value/array_data.rs`) stores data directly in Rust `Vec<T>` variants:

```rust
pub enum ArrayData {
    F64(Vec<f64>),
    I64(Vec<i64>),
    Bool(Vec<bool>),
    // ... other types
}
```

There is no indirection through Memory or MemoryRef — Array owns its data directly.

### 3.2 Current SubArray Implementation

SubArray (in `subset_julia_vm/src/julia/base/subarray.jl`) uses a simplified model:

```julia
struct SubArray{T}
    parent::Vector{T}    # Reference to parent array
    offset::Int64        # 0-indexed offset into parent
    len::Int64           # Length of the view
end
```

This stores a reference to the parent Array plus an offset — similar in spirit to MemoryRef but operating at the Array level rather than the Memory level.

## 4. Feasibility Analysis

### 4.1 What MemoryRef Enables (and Whether SubsetJuliaVM Needs It)

| Capability | Julia Official | SubsetJuliaVM Need | Priority |
|---|---|---|---|
| Zero-copy reshape | MemoryRef sharing between Arrays | Implemented for legacy VM arrays through a `shared_parent` bridge; final MemoryRef representation remains | Phase 2 |
| Efficient pushfirst!/popfirst! | Offset within overallocated Memory | Current Vec handles this internally | Low |
| wrap(Array, Memory, dims) | Creates Array from MemoryRef | Basic no-offset Memory wrapper path exists; MemoryRef offset path remains | Phase 2 |
| Atomic operations | AtomicMemoryRef | Not planned (no threading) | None |
| SubArray views | Not via MemoryRef (uses parent + indices) | Already works via parent + offset | Already done |
| Memory sharing between Arrays | Core use case | Needed for reshape, views | Phase 2 |

### 4.2 Key Insight: SubArray Does NOT Use MemoryRef

In official Julia, `SubArray` stores a reference to the **parent Array** plus indices — it does not go through MemoryRef directly. MemoryRef sharing is specifically for `Array`-to-`Array` relationships (reshape, wrap). SubsetJuliaVM's current SubArray implementation is architecturally aligned with Julia's approach.

### 4.3 When MemoryRef Becomes Necessary

MemoryRef becomes necessary when SubsetJuliaVM implements:

1. **`reshape`** — requires two Arrays to share the same underlying storage
2. **`wrap(Array, Memory, dims)`** — creates Array from raw Memory
3. **Multi-dimensional array operations** — where different views of the same data are needed
4. **Memory-level operations** — `unsafe_copyto!`, `memoryref_isassigned`, etc.

## 5. Simplified Alternative: Array Holds Memory + Offset Directly

Instead of a full `MemoryRef` type, SubsetJuliaVM could use a simplified approach where Array holds:

```rust
struct ArrayValue {
    memory: Rc<RefCell<MemoryValue>>,  // shared Memory
    offset: usize,                      // offset into memory (0-indexed)
    dims: Vec<usize>,                   // dimensions
}
```

**Pros:**
- Simpler implementation — no separate MemoryRef type in the value system
- Achieves the same sharing semantics (multiple ArrayValues can point to the same MemoryValue)
- Easier to implement in a non-JIT, interpreted VM

**Cons:**
- Not a 1:1 match with Julia's type system (users can't construct `MemoryRef` values)
- Cannot implement `memoryref()`, `memoryrefoffset()` intrinsics faithfully
- Would need refactoring if full MemoryRef support is later needed

### 5.1 Assessment

The simplified approach is viable for Phase 1 (basic Memory{T} support) because:
- No user-facing Julia code in the supported subset constructs MemoryRef explicitly
- All MemoryRef usage in official Julia is internal to Array/Memory operations
- SubsetJuliaVM can internalize the offset concept without exposing it as a type

However, for Phase 2 (full Memory compatibility), a proper MemoryRef type would be needed to support:
- `memoryref(mem)`, `memoryref(mem, i)`, `memoryref(ref, i)` functions
- `memoryrefoffset(ref)` intrinsic
- Passing MemoryRef values between functions

## 6. Recommendation

### Phase 1: Skip MemoryRef (Completed)

For the Memory{T} implementation (Issue #2765):
- **MemoryRef was NOT implemented** as a Value variant (as recommended)
- `Value::Memory(MemoryRef)` uses `Rc<RefCell<MemoryValue>>` directly (no offset)
- Array continues to own its data directly via `ArrayData`
- `Memory{T}` works as a standalone fixed-size typed vector
- VM instructions: `NewMemory`, `NewMemoryDynamic`, `MemoryGet`, `MemorySet`, `MemoryLength`
- Builtins: `memorynew`, `memoryref`, `memoryrefget`, `memoryrefset!` migrated to Rust builtins
- Dict uses Memory-based open-addressing hash table (PR #2773)

### Phase 2: Implement MemoryRef (Future)

When implementing reshape, wrap, or Memory-backed Arrays:
1. Add `Value::MemoryRef` variant to the VM
2. Refactor `ArrayValue` to hold `MemoryRef` (shared Memory + offset) instead of direct `Vec<T>`
3. Implement `memoryref()`, `memoryrefoffset()`, `memoryrefget()`, `memoryrefset!()` intrinsics
4. Enable `reshape` as zero-copy operation via shared MemoryRef

### Implementation Sketch for Phase 2

```rust
// MemoryRef: a reference into a Memory at a specific offset
pub struct MemoryRefValue {
    pub memory: Rc<RefCell<MemoryValue>>,  // shared with other refs
    pub offset: usize,                      // 1-based index into memory
}

// Array becomes:
pub struct ArrayValue {
    pub ref_: MemoryRefValue,  // replaces current ArrayData ownership
    pub dims: Vec<usize>,
}

// reshape becomes zero-copy:
fn reshape(array: &ArrayValue, new_dims: Vec<usize>) -> ArrayValue {
    assert_eq!(array.len(), new_dims.iter().product());
    ArrayValue {
        ref_: MemoryRefValue {
            memory: Rc::clone(&array.ref_.memory),
            offset: array.ref_.offset,
        },
        dims: new_dims,
    }
}
```

### Required New VM Intrinsics (Phase 2)

| Intrinsic | Signature | Description |
|---|---|---|
| `memoryrefnew` | `(Memory) -> MemoryRef` | Create ref at start |
| `memoryrefnew` | `(Memory, Int) -> MemoryRef` | Create ref at index |
| `memoryrefnew` | `(MemoryRef, Int) -> MemoryRef` | Advance ref by offset |
| `memoryrefget` | `(MemoryRef, Symbol) -> T` | Read value at ref |
| `memoryrefset!` | `(MemoryRef, T, Symbol) -> T` | Write value at ref |
| `memoryrefoffset` | `(MemoryRef) -> Int` | Get 1-based index |
| `memoryref_isassigned` | `(MemoryRef, Symbol) -> Bool` | Check if assigned |

## 7. Summary

| Question | Answer |
|---|---|
| Does SubsetJuliaVM need MemoryRef now? | **No** — not for Phase 1 Memory{T} implementation |
| When is MemoryRef needed? | When implementing reshape, wrap, or Memory-backed Arrays |
| Can we use a simplified alternative? | Yes, for Phase 1. Phase 2 should use proper MemoryRef |
| Does SubArray need MemoryRef? | No — SubArray uses parent Array + indices, not MemoryRef |
| What's the migration path? | Phase 1: standalone Memory{T} -> Phase 2: MemoryRef + refactored Array |
