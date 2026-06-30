# Issue #5882: `Base.Pairs{K,V,I,A}` declares `AbstractDict{K,V}` as its parametric
# abstract parent, so the parameterized subtype relation must thread K,V from the
# Pairs instantiation into AbstractDict{K,V}. Previously `supertype(Pairs{...})`
# returned `Any` (the builtin direct-supertype table hardcoded Pairs to `Any`,
# shadowing the pure-Julia `struct Pairs{K,V,I,A} <: AbstractDict{K,V}`), so the
# parameterized relation was false while the bare one was true.

using Test
import Base: Pairs

@testset "Pairs parametric parent threads into AbstractDict{K,V} (Issue #5882)" begin
    P = Pairs{Symbol,Int64,Tuple{Symbol},NamedTuple{(:a,),Tuple{Int64}}}

    @test (P <: AbstractDict) == true
    @test (P <: AbstractDict{Symbol,Int64}) == true
    @test (P <: AbstractDict{Symbol,Any}) == false
    @test (P <: AbstractDict{Any,Int64}) == false
    @test supertype(P) == AbstractDict{Symbol,Int64}
end

@testset "supertype regressions (Issue #5882)" begin
    @test supertype(Complex{Float64}) == Number
    @test supertype(Dict{String,Int64}) == AbstractDict{String,Int64}
    @test supertype(Vector{Int64}) == DenseVector{Int64}
end

true
