# Generalized `eltype` for any type / container (Issue #5116).
# Both the type form `eltype(T)` and the value form `eltype(x) = eltype(typeof(x))`
# must match upstream Julia exactly across arrays, dicts, sets, ranges, strings,
# tuples and the `eltype(::Type) = Any` fallback.

using Test

@testset "eltype: arrays (type + value forms)" begin
    @test eltype(Vector{Int}) === Int
    @test eltype(Matrix{Float64}) === Float64
    @test eltype(Array{Int}) === Int
    @test eltype([1, 2]) === Int
    @test eltype([1.0, 2.0]) === Float64
    @test eltype(Array) === Any
end

@testset "eltype: dicts → Pair{K,V}" begin
    @test eltype(Dict{String,Int}) === Pair{String,Int64}
    @test eltype(Dict{Int32,Float64}) === Pair{Int32,Float64}
    @test eltype(Dict("a" => 1)) === Pair{String,Int64}
    @test eltype(Dict(Int32(1) => "foo")) === Pair{Int32,String}
    @test eltype(Dict()) === Pair{Any,Any}
    d = Dict{Int,String}()
    @test eltype(d) === Pair{Int,String}
end

@testset "eltype: sets" begin
    @test eltype(Set{Int}) === Int
    @test eltype(Set([1, 2])) === Int
    @test eltype(Set(["a", "b"])) === String
end

@testset "eltype: ranges" begin
    @test eltype(1:3) === Int
    @test eltype(1:2:9) === Int
    @test eltype(UnitRange{Int}) === Int
    @test eltype(StepRange{Int,Int}) === Int
end

@testset "eltype: strings → Char" begin
    @test eltype("abc") === Char
    @test eltype(String) === Char
    @test eltype(typeof("abc")) === Char
end

@testset "eltype: tuples (typejoin of element types)" begin
    @test eltype((1, 2, 3)) === Int
    @test eltype((1, 2.0)) === Real
    @test eltype(Tuple{Int,Float64}) === Real
    @test eltype(Tuple{Int,Int}) === Int
    @test eltype(Tuple{Int,String}) === Any
    @test eltype(Tuple{}) === Union{}
    @test eltype(NTuple{3,Int}) === Int
end

@testset "eltype: scalar / fallback" begin
    @test eltype(Int) === Int
    @test eltype(1) === Int
    @test eltype(1.0) === Float64
end

true
