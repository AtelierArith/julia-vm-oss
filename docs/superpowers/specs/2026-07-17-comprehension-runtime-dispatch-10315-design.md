# Issue #10315 comprehension runtime dispatch design

## Context

An untyped comprehension collects an unresolved body through runtime type-join.
For example, `[f(i) for i in 1:3]` can become `Vector{Int64}` even when the
compiler conservatively infers `f(i)` as `Any`. The comprehension compiler
nevertheless returned `ArrayOf(Any, None)`, which the Julia-type bridge treated
as a proven `Vector{Any}`. A later overloaded call therefore bound statically to
`g(::Vector{Any})` instead of dispatching on the concrete runtime vector.

The runtime value and `typeof` were already correct; only the compiler-side
provenance used by dispatch was wrong.

## Considered approaches

1. Treat every statically inferred `Vector{Any}` argument as runtime-unknown.
   Rejected: explicit `Any[...]`, `[]`, and other proven `Vector{Any}` values
   must remain concrete dispatch inputs.
2. Give every one-dimensional comprehension a rank-bearing unknown type.
   Rejected: typed comprehensions and statically known element types already
   carry exact information and do not need runtime deferral.
3. Preserve known rank only for comprehensions that emit `ArrayPushTypejoin`.
   Selected: this is the exact point where the runtime element type may narrow
   away from the compiler placeholder.

## Design

When an untyped single-iterator comprehension uses runtime type-join, return
`ValueType::ArrayOf(element_placeholder, Some(1))` from both its indexed fast
path and iteration-protocol path. The existing Julia-type bridge recognizes
that shape as rank-known but element-unknown and reports bare `Vector`; the
existing dispatch policy then defers overloaded array-family calls to runtime.

Tuple-destructuring comprehensions always collect through runtime type-join, so
their internal empty-union sentinel is exposed as an unresolved `Any` element
with the same rank-1 provenance. Forced runtime element types
continue to return the generic `Array` type, and typed/static-element
comprehensions retain their existing exact result type. No dispatch matcher or
runtime method-selection rule changes.

## Verification

Add an upstream-parity fixture covering variable-bound and inline range
comprehensions, the non-indexable iteration-protocol path, tuple destructuring,
a genuinely heterogeneous `Vector{Any}`, and an explicitly typed `Any`
comprehension. Run the fixture red before implementation, then the focused
compiler tests, dispatch fixtures, formatting/clippy gates, and the full suite
before merge.
