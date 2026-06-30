# Dict Indexing Architecture (Issue #1820)

This document describes how Dict indexing is compiled and executed in SubsetJuliaVM.

## Overview

Dict indexing follows three code paths depending on compile-time type
information and representation.

## Struct-backed `Dict{K,V}` Path

When public construction produces the Pure Julia `Dict{K,V}` struct:
- `d[key]` compiles through `getindex(::Dict{K,V}, key)` method dispatch
- `d[key] = val` compiles through `setindex!(::Dict{K,V}, val, key)` method dispatch
- Key/value type information is preserved through the `Dict{K,V}` fields

This is the normal public route after #6619/#6621.

## Legacy `Value::Dict` Path

When the compiler/runtime is handling a legacy Rust-backed `Value::Dict`
boundary value:
- `d[key]` may use `CallBuiltin(DictGet)`
- `d[key] = val` may use `CallBuiltin(DictSet)`
- `NewDict*`, `LoadDict`, `StoreDict`, `ReturnDict`, and public Dict builtins
  are retained for old bytecode/cache compatibility and VM-boundary fallback

## Runtime Path (Any-typed parameters)

When the compiler infers `Any` (e.g., `function f(d) ... d[key] ... end`):
- `d[key]` emits `IndexLoad` -> runtime checks target type and dispatches to Dict fallback
- `d[key] = val` emits `IndexStore` -> runtime checks target type and dispatches to Dict fallback
- The `IndexLoad`/`IndexStore` handlers in `vm/exec/array_index.rs` must handle Dict as a potential target

## Dict Key Types

`DictKey` in `vm/value/container.rs` supports:
- `DictKey::Str(String)` -- String keys
- `DictKey::I64(i64)` -- Integer keys
- `DictKey::Symbol(String)` -- Symbol keys (`:a`, `:b`, etc.)

## Code Review Checklist for Dict/Indexing Changes

- [ ] When modifying `IndexLoad`/`IndexStore`, always consider Dict as a potential target type
- [ ] When adding new `DictKey` variants, update all match sites: `container.rs`, `sjulia.rs` (display), `builtins_types.rs` (comparison), `call.rs` (kwargs expansion)
- [ ] When the compiler infers `Any`, verify indexing defers to runtime dispatch (not array-only assumptions)
- [ ] Test Dict indexing through both typed (`d::Dict`) and untyped (`d`) function parameters

## Pure Julia Migration (Issue #6571)

The public `Dict` surface is being migrated toward the Pure Julia `Dict{K,V}`
struct in `base/dict.jl`, leaving `Value::Dict` as a VM boundary / cache
fallback. After #6621, new public struct-backed `Dict{K,V}` indexing no longer
emits `CallBuiltin(DictGet/DictSet)`; those fast paths are classified as the
**primitive `Value::Dict` fallback** and old bytecode/cache boundary. See the
*Dict → Pure Julia Migration Audit (Issue #6571)* section in
`BUILTIN_REMOVAL.md` for the full handler classification and roadmap.

## Relation to Array Native-Carrier Demotion (Issue #6653)

The final Array migration follows the same boundary policy as Dict: public
routes use the Pure Julia struct representation, while the native carrier stays
available only as an old-bytecode/cache and VM-boundary fallback. For Dict this
means `Dict{K,V}` owns public construction and indexing while `Value::Dict`
handlers remain for primitive boundary values. For Array, #6653 makes public
construction/materialization/HOF/broadcast routes return `Array{T,N}` wrappers
backed by `MemoryRef{T}`, while `Value::NativeArray` remains a compatibility
carrier for VM instructions, host/REPL/formatting boundaries, and old caches.

The same performance rule applies to both migrations: benchmark the struct route
and optimize `Memory`/method hot paths, but do not restore the native carrier as
the public default. Dict numbers are tracked by `vm_dict_benchmark` (#6622);
Array numbers are tracked by `vm_array_benchmark` (#6653).

## Related Documentation

- `BUILTIN_REMOVAL.md` - Dict handler classification & migration roadmap (Issue #6571)
- `TYPE_SYSTEM.md` - Type system architecture
- `CLAUDE.md` - Top-level contributor guidelines
