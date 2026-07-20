# Pair diagonal UnionAll keeps both invariant parameter slots (Issue #10635)

using Test

@testset "Pair diagonal UnionAll preserves repeated type parameter" begin
    x = Pair{T,T} where T
    y = Pair{S,S} where S
    generic = Pair{K,V} where {K,V}
    bounded = Pair{K,V} where {K<:Integer,V<:Real}

    @test string(x) == "Pair{T, T} where T"
    @test string(y) == "Pair{S, S} where S"
    @test string(generic) == "Pair"
    @test generic == Pair
    @test string(bounded) == "Pair{K, V} where {K<:Integer, V<:Real}"
    @test Pair{Int64,Float64} <: bounded
    @test !(Pair{Float64,Float64} <: bounded)

    @test x == y
    @test x <: y
    @test y <: x

    @test Pair{Int64,Int64} <: x
    @test !(Pair{Int64,String} <: x)
end

true
