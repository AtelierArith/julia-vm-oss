# Issue #6657: `first`/`last` called through an `Any`-typed binding must reach a
# user-defined `first(::Vector{Int64})` / `last(::Vector{Int64})` override,
# instead of being coerced to the array element type. The wrapper's return type
# was inferred via the per-call-site single-function engine, which could not see
# the user override's method table and fell back to the element-type tfunc,
# emitting a typed return that crashed on the override's non-element value
# (`expected I64, got Symbol`). The engine now seeds user-overridden method
# tables so the override's declared return type is visible. Verified against
# upstream Julia 1.12.
#
# Scope: covers the `first`/`last` cases of #6657. The `getindex` (`xs[1]`) case
# lowers to the `IndexLoad` fast path and needs separate dispatch infrastructure;
# it remains tracked under #6657.

using Test

import Base: first, last

first(xs::Vector{Int64}) = :ov_first
last(xs::Vector{Int64}) = :ov_last

# Receiver is Any-typed, so the override must be reached at runtime.
call_first(xs) = first(xs)
call_last(xs) = last(xs)

@testset "first/last(::Any) dispatch to user Vector methods (#6657)" begin
    @test call_first([10, 20, 30]) == :ov_first
    @test call_last([10, 20, 30]) == :ov_last

    # Non-overridden element types keep the builtin element-returning behavior.
    @test first([1.0, 2.0]) === 1.0
    @test last([1.0, 2.0]) === 2.0
end

# Final value gates the in-harness nextest run on correctness, not just no-throw.
call_first([10, 20, 30]) == :ov_first &&
    call_last([10, 20, 30]) == :ov_last &&
    first([1.0, 2.0]) === 1.0 &&
    last([1.0, 2.0]) === 2.0
