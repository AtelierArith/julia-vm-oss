# RangeValue Migration Plan

Issue #10150 tracks retiring `RangeValue` as the long-lived representation for
Julia ranges. The target shape is that range values are ordinary Julia structs
whose behavior is supplied by Base methods.

## Target Model

- `UnitRange{T}` is a Julia struct with `start::T` and `stop::T`.
- `StepRange{T,S}` is a Julia struct with `start::T`, `step::S`, and `stop::T`.
- `StepRangeLen` and `LinRange` remain Julia structs.
- Parser-created range literals eventually lower to constructor calls instead
  of pushing native `Value::Range`.
- Public operations (`first`, `last`, `step`, `length`, `getindex`, `iterate`,
  `collect`, `eltype`, `IteratorSize`, `IteratorEltype`) should resolve through
  Julia methods for struct-backed ranges.

## Current Bridge

This migration starts by defining `UnitRange` and `StepRange` in
`subset_julia_vm/src/julia/base/range.jl` while keeping existing colon literals
on the native `Value::Range` path.

During the bridge period, a few Rust builtins intentionally accept both
representations:

- `_tuple_first` reads native range starts, struct `start` fields, and
  `OneTo`'s implicit start value.
- `_tuple_last` reads native range stops, struct `stop` fields, and `OneTo.stop`.
- `_range_step` reads native range steps, struct `StepRange.step`, and
  synthesizes typed `oneunit` for struct `UnitRange` plus `1` for `OneTo`.

That keeps function-value calls such as `f = step; f(r)` working for both old
and new range values while later phases move colon lowering.

## Reduction Steps

1. Keep direct `UnitRange(...)` and `StepRange(...)` constructor fixtures green
   for Int64, BigInt, Float64, Float32, Char, and narrow unsigned ranges.
2. Done: parametric abstract range dispatch (`AbstractRange{T}`) now binds `T`
   for native `UnitRange`/`StepRange`, direct struct constructors, BigInt,
   narrow unsigned, Char, and Float32 `StepRangeLen` fixtures (#10150).
3. In progress: ordinary expression-position colon literals now call Julia
   constructors for statically known non-float ranges (`UnitRange` /
   `StepRange`) covering integer, BigInt, Char, and narrow integer operands.
   Float colon ranges stay on the native `StepRangeLen` path for
   TwicePrecision-compatible semantics. For-head/comprehension coercion paths
   and runtime-unknown ranges still use `MakeRangeLazy` / `MakeStepRangeLazy`.
4. Keep floating colon ranges on `StepRangeLen` with the existing
   TwicePrecision-compatible semantics.
5. Once colon values are struct-backed, remove `RangeElementType`,
   `derive_range_element_type`, `derive_range_step_type`, and native
   `RangeValue` accessor/collect fast paths that duplicate Base methods.
6. Leave a native range representation only if a backend-specific boundary
   needs it; such use must be private and not observable as public Julia range
   behavior.
