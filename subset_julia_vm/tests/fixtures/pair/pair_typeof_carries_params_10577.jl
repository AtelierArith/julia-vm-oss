# Issue #10577: typeof(::Pair) must carry its concrete element parameters
# (Pair{Int64, Float64}) so parametric isa/<: queries on a Pair value resolve.
# Base `Pair` is modeled non-parametrically in sjulia, so the value-side
# typeof/isa projection has to reconstruct Pair{typeof(first), typeof(second)}.
using Test

@testset "typeof(::Pair) carries concrete parameters" begin
    p = 1 => 2.0
    @test typeof(p) === Pair{Int64,Float64}
    @test typeof(Pair(1, 2)) === Pair{Int64,Int64}
    @test typeof("a" => 1) === Pair{String,Int64}
    # nested Pair projects recursively, not Pair{Pair, Int64}
    @test typeof(Pair(1 => 2, 3)) === Pair{Pair{Int64,Int64},Int64}
end

@testset "parametric isa/<: on a Pair value" begin
    p = 1 => 2.0
    r = Pair{A,B} where {A<:Real,B<:Real}
    @test p isa r
    @test p isa Pair{Int64,Float64}
    @test typeof(p) <: r
    # bare-family and non-matching concrete membership stay correct
    @test p isa Pair
    @test !(p isa Pair{Int64,Int64})
end

true
