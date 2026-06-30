# Core.apply_type + splat in parametric type construction (Issue #5112)
# Construct parametric types from computed/splatted type values.

using Test

struct AT5112Box{T}
    x::T
end

@testset "Core.apply_type with computed type values" begin
    @test Core.apply_type(Tuple, Int, Real) === Tuple{Int64,Real}
    @test Core.apply_type(Tuple, Int) === Tuple{Int64}
    @test Core.apply_type(Array, Int, 1) === Vector{Int64}
    @test Core.apply_type(Vector, Int) === Vector{Int64}
    @test Core.apply_type(AT5112Box, Int) === AT5112Box{Int64}
end

@testset "splat in T{xs...}" begin
    ts = [Int, Real]
    @test Tuple{ts...} === Tuple{Int64,Real}

    tt = (Int, Real)
    @test Tuple{tt...} === Tuple{Int64,Real}

    # Leading static arg followed by a splat
    gg = [Real]
    @test Tuple{Int,gg...} === Tuple{Int64,Real}

    # Variable-arity through a vararg function
    f(args...) = Tuple{args...}
    @test f(Int, Real) === Tuple{Int64,Real}
    @test f(Int) === Tuple{Int64}
end

true
