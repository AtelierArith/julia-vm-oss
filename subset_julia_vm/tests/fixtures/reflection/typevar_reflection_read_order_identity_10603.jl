# Read-order independence of UnionAll TypeVar reflection identity (Issue #10603).
# Wrapper-chain projections (`.var`, body `.parameters`) must resolve to the
# wrapper's OWN owner-scoped identity, and constructed parametric type arguments
# must resolve to the recorded user TypeVar — regardless of which reflection
# field is read first. Verified against julia 1.12.

using Test

# Direction A: reading a wrapper body `.parameters` BEFORE `.var`, after a user
# `Vector{T}` construction populated the constructed-TypeVar cache, must NOT
# leak the user's `T` into the wrapper chain.
@testset "reflection: Direction A — Vector body-params-before-var is order independent (Issue #10603)" begin
    T = TypeVar(:T)
    @test Vector{T}.parameters[1] === T
    p1 = Vector.body.parameters[1]        # body .parameters read BEFORE .var
    @test p1 !== T
    @test p1 === Vector.var
    @test p1 isa TypeVar
end

@testset "reflection: Direction A — Matrix body-params-before-var is order independent (Issue #10603)" begin
    S = TypeVar(:S)
    @test Matrix{S}.parameters[1] === S
    m1 = Matrix.body.parameters[1]
    @test m1 !== S
    @test m1 === Matrix.var
    @test m1 isa TypeVar
end

# Direction B: reading a wrapper `.var` BEFORE constructing `Dict{K,V}` /
# `Set{T}` must NOT leak the wrapper projection into the constructed type's
# `.parameters` — those must still be the user's TypeVars.
@testset "reflection: Direction B — Dict var-before-construction is order independent (Issue #10603)" begin
    K = TypeVar(:K)
    V = TypeVar(:V)
    dv = Dict.var                          # wrapper projection read FIRST
    @test Dict{K, V}.parameters[1] === K
    @test Dict{K, V}.parameters[2] === V
    @test dv !== K
end

@testset "reflection: Direction B — Set var-before-construction is order independent (Issue #10603)" begin
    T = TypeVar(:T)
    sv = Set.var                           # wrapper projection read FIRST
    @test Set{T}.parameters[1] === T
    @test sv !== T
end

true
