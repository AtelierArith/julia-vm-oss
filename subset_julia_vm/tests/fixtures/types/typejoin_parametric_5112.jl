# typejoin for parametric Tuple types and same-name parametric structs (Issue #5112)

using Test

struct TJ5112Box{T}
    x::T
end

@testset "typejoin - parametric Tuple types (Issue #5112)" begin
    # Elementwise join of fixed-length Tuple types
    @test typejoin(Tuple{Int}, Tuple{Float64}) === Tuple{Real}
    @test typejoin(Tuple{Int,Int}, Tuple{Int,Float64}) === Tuple{Int64,Real}
    # Identical tuples are unchanged
    @test typejoin(Tuple{Int}, Tuple{Int}) === Tuple{Int64}
    @test typejoin(Tuple{Int,String}, Tuple{Int,String}) === Tuple{Int64,String}
end

@testset "typejoin - same-name parametric structs (Issue #5112)" begin
    # Differing parameters collapse to the base type
    @test typejoin(TJ5112Box{Int}, TJ5112Box{Float64}) === TJ5112Box
    # Identical instantiations are unchanged
    @test typejoin(TJ5112Box{Int}, TJ5112Box{Int}) === TJ5112Box{Int}
end

@testset "typejoin - existing scalar behaviour preserved" begin
    @test typejoin(Int64, Float64) === Real
    @test typejoin(Int64, Int64) === Int64
    @test typejoin(Int64, String) === Any
end

true
