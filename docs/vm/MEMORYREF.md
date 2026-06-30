# MemoryRef Status

*Last updated: 2026-06-14*

This page is the active status note for MemoryRef. The original Issue #2765
feasibility investigation is preserved in
`docs/vm/archived/MEMORYREF_INVESTIGATION_20260611.md`.

## Current State

SubsetJuliaVM now has both:

- `Value::Memory(MemoryRef)` — a flat typed `Memory{T}` buffer backed by
  `MemoryValue`.
- `Value::MemoryRef(Box<MemoryRefValue>)` — an offset reference into
  `Memory{T}`, used by `memoryref`, `memoryrefnew`, `memoryrefget`,
  `memoryrefset!`, `memoryrefoffset`, and `memoryrefparent`.

`Value::Array` has been retired. The remaining compatibility carrier is
`Value::NativeArray(ArrayRef)`, with all compatibility boundaries routed through
explicit `native_array_*` helpers. Public Array construction/materialization and
HOF/broadcast routes now return `Array{T,N}` wrappers over `MemoryRef{T}`;
`NativeArray` is retained for old cache bytecode, VM fallback, and host/REPL/
formatting boundaries. See `ARRAY_MEMORY_MIGRATION.md` for the current
retirement plan and inventory.

Legacy VM-array `reshape` sharing is currently handled by the `ArrayValue`
`shared_parent` bridge. That preserves zero-copy behavior for the native-array
compatibility carrier while remaining internal call sites move toward the
Julia-visible `Array{T,N}` over `MemoryRef{T}` representation.

## Update Guide

- For the active array/memory migration inventory, update
  `docs/vm/ARRAY_MEMORY_MIGRATION.md`.
- For the original Memory primitive architecture and phase checklist, update
  `docs/vm/MEMORY_PRIMITIVE.md`.
- For upstream compatibility decisions, study `julia/src/jltypes.c`,
  `julia/src/array.c`, `julia/base/essentials.jl`, and
  `julia/base/array.jl` before changing `Memory`, `MemoryRef`, or
  `Array{T,N}` behavior.
