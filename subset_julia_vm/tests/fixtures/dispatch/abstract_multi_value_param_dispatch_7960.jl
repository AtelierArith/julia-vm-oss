using Test

# Issue #7960: method dispatch on an ABSTRACT type parameterized by integer
# VALUE parameters, called through a CONCRETE subtype, previously dropped every
# parameter to the bare family name (`AbsM{2,2,T}` -> `AbsM`). All value-parameter
# specializations therefore collapsed into one indistinguishable signature, so
# the last-defined one always won regardless of the actual values: `h(ConM{2,2})`
# wrongly selected the `AbsM{3,3,T}` method. The concrete subtype's value
# parameters are now projected up to the abstract supertype's instantiation
# (`ConM{2,2,Float64}` -> `AbsM{2,2,Float64}`) and compared, so the correct
# specialization is selected and the parametric method outranks the generic one.

abstract type AbsM{M,N,T} end
struct ConM{M,N,T} <: AbsM{M,N,T}
    data::Tuple
end

h(x::AbsM) = "generic"
h(x::AbsM{2,2,T}) where {T} = "spec-2x2"
h(x::AbsM{3,3,T}) where {T} = "spec-3x3"

@testset "concrete subtype selects the matching value-parameter specialization" begin
    @test h(ConM{2,2,Float64}((1.0,))) == "spec-2x2"
    @test h(ConM{3,3,Float64}((1.0,))) == "spec-3x3"
    # A size with no specialization falls back to the generic method.
    @test h(ConM{4,4,Float64}((1.0,))) == "generic"
end

# The specialization must outrank the generic method regardless of definition
# order (the fix ranks `AbsM{2,2,T}` strictly above the bare `AbsM`, rather than
# relying on "last defined wins").
abstract type AbsR{M,N,T} end
struct ConR{M,N,T} <: AbsR{M,N,T}
    data::Tuple
end

r(x::AbsR{2,2,T}) where {T} = "r-2x2"
r(x::AbsR{3,3,T}) where {T} = "r-3x3"
r(x::AbsR) = "r-generic"

@testset "specialization outranks the generic even when defined first" begin
    @test r(ConR{2,2,Float64}((1.0,))) == "r-2x2"
    @test r(ConR{3,3,Float64}((1.0,))) == "r-3x3"
    @test r(ConR{4,4,Float64}((1.0,))) == "r-generic"
end

# A single integer value parameter on the abstract supertype dispatches just as
# correctly as the multi-parameter case.
abstract type AbsV{N,T} end
struct ConV{N,T} <: AbsV{N,T}
    data::Tuple
end

g(x::AbsV) = "g-generic"
g(x::AbsV{2,T}) where {T} = "g-2"
g(x::AbsV{3,T}) where {T} = "g-3"

@testset "single value parameter on abstract supertype" begin
    @test g(ConV{2,Int}((1,))) == "g-2"
    @test g(ConV{3,Int}((1,))) == "g-3"
end

true
