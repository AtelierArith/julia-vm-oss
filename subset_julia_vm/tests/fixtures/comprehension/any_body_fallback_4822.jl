# Issue #4822: when a comprehension body's inferred type was `Any`
# (e.g. `convert(Any, x)`, or any call returning `Any`), the result
# silently became `Vector{Float64}` with each element coerced to Float.
#
# Root cause: `compile_comprehension` had a fallback at the end of its
# element-type chain that defaulted to `ArrayElementType::F64` when
# body inference yielded a non-Tuple `Any`. The fallback existed for
# the "everything-is-numeric" optimization era; with richer inference
# today it silently corrupted non-numeric data and produced a wrong
# typeof.
#
# Fix: change the fallback to `ArrayElementType::Any` so an
# Any-typed body produces a `Vector{Any}` carrying the source values
# without coercion. Upstream Julia often infers tighter (e.g.
# `Vector{Int64}` for `[convert(Any, x) for x in [1, 2, 3]]`), but
# `Vector{Any}` is the safe, lossless default and matches what
# upstream emits when inference cannot do better.

using Test

@testset "convert(Any, x) body: no Float coercion (Issue #4822)" begin
    v = [convert(Any, x) for x in [1, 2, 3]]
    # Values must be preserved verbatim (not coerced to Float).
    @test v[1] === 1
    @test v[2] === 2
    @test v[3] === 3
    @test v == [1, 2, 3]
    # Eltype must NOT silently widen to Float64.
    @test eltype(v) !== Float64
end

@testset "convert(Any, x) body: String source not coerced (Issue #4822)" begin
    v = [convert(Any, x) for x in ["a", "b"]]
    @test v == ["a", "b"]
    @test eltype(v) !== Float64
end

@testset "non-Any comprehension still F64 when body is Float (Issue #4822)" begin
    # Regression guard: when body inference is concrete and Float64,
    # the result must remain a Float64 vector (this path was unaffected
    # by the fallback change).
    v = [Float64(x) for x in [1, 2, 3]]
    @test typeof(v) === Vector{Float64}
    @test v == [1.0, 2.0, 3.0]
end

@testset "untyped int identity comprehension still Vector{Int64} (Issue #4822)" begin
    # Regression guard: the iter-eltype path picks up Int64.
    v = [x for x in [1, 2, 3]]
    @test typeof(v) === Vector{Int64}
end

true
