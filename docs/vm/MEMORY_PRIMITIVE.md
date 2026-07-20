# Memory Primitive Status

*Last updated: 2026-06-11*

This page is the active status note for `Memory{T}` as a VM primitive. The
original Issue #2756 architecture design is preserved in
`docs/vm/archived/MEMORY_PRIMITIVE_ARCHITECTURE_20260611.md`.

## Current State

SubsetJuliaVM has a Rust-backed `Memory{T}` primitive:

- `Value::Memory(MemoryRef)` stores a shared `MemoryValue`.
- `MemoryValue` is a flat typed buffer backed by `ArrayData` plus an
  `ArrayElementType`.
- `NewMemory`, `NewMemoryDynamic`, `MemoryGet`, `MemorySet`, `MemoryLength`,
  `LoadMemory`, and `StoreMemory` handle VM-level memory operations.

SubsetJuliaVM also has an offset `MemoryRef` value:

- `Value::MemoryRef(Box<MemoryRefValue>)` represents a reference into a
  `Memory{T}` buffer.
- `memoryref`, `memoryrefnew`, `memoryrefget`, `memoryrefset!`,
  `memoryrefoffset`, and `memoryrefparent` route through Rust builtin
  boundaries.
- `subset_julia_vm/src/julia/base/genericmemory.jl` provides the small
  Pure Julia glue layer for the supported public helpers.

`Value::Array` has been retired. The remaining native-array compatibility
carrier is `Value::NativeArray(ArrayRef)`, and public array behavior continues
moving toward Memory primitives plus Julia-visible `Array{T,N}` wrappers. The
active migration inventory and remaining retirement plan are in
`docs/vm/ARRAY_MEMORY_MIGRATION.md`, not this file.

## Active Owners

- `docs/vm/ARRAY_MEMORY_MIGRATION.md` — current native-array compatibility
  retirement plan, memory-first array construction work, and audit inventory.
- `docs/vm/MEMORYREF.md` — current `MemoryRef` status and the archived
  feasibility investigation.
- `docs/vm/CODE_AUDITS.md` — audit policy for the zero-match
  `Value::Array` check and memory-first array construction guards.
- `subset_julia_vm_vm/src/vm/value/memory_value.rs` — `MemoryValue` and
  `MemoryRefValue`.
- `subset_julia_vm_vm/src/vm/value/value_enum.rs` — runtime `Value::Memory` and
  `Value::MemoryRef` variants.
- `subset_julia_vm_vm/src/vm/exec/memory.rs` — VM memory instruction execution.
- `subset_julia_vm_vm/src/vm/builtins_collections.rs` — `memoryref*` builtin
  execution.
- `subset_julia_vm_compile/src/compile/expr/builtin.rs` — compile-time routing for
  `memoryref*` helpers.
- `subset_julia_vm/src/julia/base/genericmemory.jl` — supported Julia-facing
  memory helper definitions.

## Upstream References

Before changing `Memory`, `MemoryRef`, or array wrapper behavior, compare with:

- `julia/src/jltypes.c`
- `julia/src/array.c`
- `julia/base/essentials.jl`
- `julia/base/array.jl`
