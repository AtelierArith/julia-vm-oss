using Test

# Issue #4698: a fresh TypeVar embedded in a parametric type's `.parameters`
# must remain `===` to the original TypeVar object, not just name-equal.
@testset "parametric TypeVar identity (Issue #4698)" begin
    T = TypeVar(:T)

    # Vector{T}.parameters[1] is the *same* TypeVar object as T.
    @test Vector{T}.parameters[1] === T
    @test Matrix{T}.parameters[1] === T

    # Distinct TypeVars keep distinct identity.
    S = TypeVar(:S)
    @test Vector{S}.parameters[1] === S
    @test !(Vector{S}.parameters[1] === T)
    @test !(Vector{T}.parameters[1] === S)

    # Reconstructing the same parametric type recovers the same TypeVar.
    @test Vector{T}.parameters[1] === Vector{T}.parameters[1]

    # The recovered parameter is a TypeVar, isa Any, and prints like upstream.
    p = Vector{T}.parameters[1]
    @test p isa TypeVar
    @test isequal(p, T)
end

true
