# AoT Rooting and Safepoint Contract

**Last updated**: 2026-06-24 (Issue #7104, Cranelift GC/rooting contract)

This document defines the current ownership/rooting contract for runtime `Value`
objects in AoT output. It is not a garbage collector implementation. The goal is
to make the native AoT boundary explicit before Cranelift or runtime-helper
coverage grows.

## Value Classes

`subset_julia_vm/src/aot/rooting.rs` classifies values into:

| Class | Meaning |
|-------|---------|
| `Native` | Plain backend value with no runtime `Value` obligation. |
| `RuntimeOwned` | Owned runtime `Value`. Current generated Rust uses this for `Any` / boxed dynamic results. |
| `RuntimeBorrowed` | Borrowed runtime data that cannot survive an allocating helper unrooted. |
| `Rooted` | Runtime data explicitly rooted across safepoints. |
| `Temporary` | Runtime value that must be consumed before the next safepoint. |

The existing Rust AoT backend is conservative: dynamic arguments/results are
owned `Value`s. That means `Value::Str`, `Value::NativeArray`,
`Value::Memory`, `Value::MemoryRef`, `Value::Dict`, and `Value::Struct` own
their Rust-managed heap handles (`String`, `Rc<RefCell<_>>`, or `Vec<Value>`)
rather than exposing borrowed raw pointers.

## Helper Effects

Helper calls are classified as:

| Effect | Meaning |
|--------|---------|
| `NonAllocating` | Does not allocate and is not a safepoint for runtime `Value` liveness. |
| `AllocatingSafepoint` | May allocate or call runtime dispatch. Borrowed runtime values must be rooted/owned. |
| `UnknownSafepoint` | Effect is not modeled yet. Native backends must treat it as a safepoint. |

Dynamic calls and dynamic binary operations are `AllocatingSafepoint`.
Allocation-like builtins such as `collect`, `zeros`, `ones`, `map`, `filter`,
`reduce`, mutating vector growth helpers, `string`, and `linspace` are also
allocating safepoints. IO/random helpers are treated conservatively as unknown
safepoints.

## Verifier Rule

`verify_aot_rooting_obligations(stage, program)` runs from the named AoT pass
verifier. It rejects a `RuntimeBorrowed` or `Temporary` value that is live across
an allocating/unknown helper unless that value is `RuntimeOwned` or `Rooted`.

Current generated Rust normally produces `RuntimeOwned` values, so the verifier
documents and enforces the intended contract without changing runtime behavior.
Future ABI-lowered IR can feed explicit borrowed/rooted obligations into
`verify_rooting_plan`.

## Cranelift Boundary

The Cranelift backend has a separate GC/rooting design contract in
[CRANELIFT_GC_ROOTING_CONTRACT.md](../aot/CRANELIFT_GC_ROOTING_CONTRACT.md).
The current implementation supports native scalars, native stack aggregates
whose fields are scalars, and read-only data-section pointers such as local
String literal payloads. These do not require GC roots.

Managed runtime pointers are still gated: heap strings, arrays, heap structs,
`Any`, multi-variant `Union`, exceptions, and runtime `Value` objects require an
explicit `SjuliaGcContext`, root slots, safepoints, and stack maps before they
can enter Cranelift codegen. The backend must keep rejecting those values with a
clear unsupported diagnostic until the follow-up implementation issues land.
