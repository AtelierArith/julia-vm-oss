# Regression guard for capture-avoiding type-variable substitution (Issue #5054).
# The internal `instantiate`/`substitute` machinery was made capture-avoiding;
# this fixture confirms ordinary parametric instantiation (the common,
# non-capturing path) keeps producing the right concrete types and dispatch.
using Test

struct Box{T}
    value::T
end

# Parametric method whose body reuses the type variable.
unwrap(b::Box{T}) where {T} = b.value
boxtype(::Box{T}) where {T} = T

# Multi-parameter parametric struct.
struct Pair2{A,B}
    first::A
    second::B
end
firsttype(::Pair2{A,B}) where {A,B} = A
secondtype(::Pair2{A,B}) where {A,B} = B

@testset "parametric instantiation regression (Issue #5054)" begin
    # Builtin parametric instantiation.
    @test Vector{Int}([1, 2, 3]) == [1, 2, 3]
    @test typeof(Vector{Int}([1, 2, 3])) === Vector{Int}
    @test Dict{String,Int} <: AbstractDict
    @test Tuple{Int,String} <: Tuple

    # User parametric struct instantiation + parametric dispatch.
    b = Box{Int}(7)
    @test b.value == 7
    @test typeof(b) === Box{Int}
    @test unwrap(b) == 7
    @test boxtype(b) === Int

    # Distinct type-variable names on a multi-parameter struct must not collide.
    p = Pair2{Int,String}(1, "x")
    @test firsttype(p) === Int
    @test secondtype(p) === String
    @test typeof(p) === Pair2{Int,String}
end

true
