using Test

@testset "UnionAll(var, body) constructor (Issue #4694)" begin
    T = TypeVar(:T)

    # When the body does not reference the bound variable, return the body
    # unchanged (matches upstream `jl_type_unionall`).
    @test UnionAll(T, Int64) === Int64
    @test UnionAll(T, Vector{Int64}) === Vector{Int64}
    @test !isa(UnionAll(T, Vector{Int64}), UnionAll)

    # Bounded TypeVars are accepted as the var argument
    S = TypeVar(:S, Union{}, Integer)
    @test UnionAll(S, Float64) === Float64
end

@testset "Base.rewrap_unionall round-trips Base.unwrap_unionall (Issue #4694)" begin
    # Substituting a concrete type strips all UnionAll layers, because the
    # concrete type does not reference the bound variables.
    @test Base.rewrap_unionall(Int64, Int64) === Int64
    @test Base.rewrap_unionall(Int64, Vector) === Int64
    @test Base.rewrap_unionall(Int64, Dict) === Int64

    # Re-wrapping the unwrapped body restores a UnionAll. The nested Dict
    # case stays a UnionAll (the body still has `K` and `V` references).
    @test isa(Base.rewrap_unionall(Base.unwrap_unionall(Vector), Vector), UnionAll)
    @test isa(Base.rewrap_unionall(Base.unwrap_unionall(Dict), Dict), UnionAll)
end

true
