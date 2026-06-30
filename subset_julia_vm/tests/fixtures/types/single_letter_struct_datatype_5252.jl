# Single uppercase-letter struct/abstract names must classify as DataType,
# not TypeVar (Issue #5252). Multi-letter names already worked; this guards
# the disambiguation that a *declared* type name resolves to a DataType
# regardless of length, while undefined single letters stay type variables.

using Test

struct P; x::Int64; y::Int64; end
struct T; v::Int64; end
struct AB; x::Int64; end
abstract type N end
struct M <: N; w::Int64; end

@testset "single-letter struct names are DataType, not TypeVar (Issue #5252)" begin
    # Classification: typeof / isa
    @assert isa(P, DataType)
    @assert isa(T, DataType)
    @assert isa(AB, DataType)
    @assert typeof(P) === DataType
    @assert typeof(T) === DataType
    @assert typeof(AB) === DataType
    @assert !isa(P, TypeVar)
    @assert !isa(T, TypeVar)

    # Concreteness / bits-layout reflection
    @assert isconcretetype(P)
    @assert isconcretetype(T)
    @assert isbitstype(P)
    @assert isbitstype(T)
    @assert sizeof(P) == 16
    @assert sizeof(T) == 8
    @assert fieldnames(P) == (:x, :y)
    @assert fieldnames(T) == (:v,)

    # Single-letter abstract type stays abstract; its concrete child is concrete
    @assert isabstracttype(N)
    @assert !isconcretetype(N)
    @assert isconcretetype(M)
    @assert isbitstype(M)

    # Construction, field access, and instance typing
    p = P(3, 4)
    @assert typeof(p) === P
    @assert p.x == 3
    @assert p.y == 4
    @assert isa(p, P)
    @assert p isa P

    t = T(7)
    @assert typeof(t) === T
    @assert t.v == 7
    @assert isa(t, T)

    m = M(9)
    @assert typeof(m) === M
    @assert isa(m, N)
    @assert m isa N

    # Dispatch on single-letter concrete types and abstract supertype
    f(::P) = 1
    f(::T) = 2
    f(::Int) = 3
    g(::N) = 100
    @assert f(p) == 1
    @assert f(t) == 2
    @assert f(5) == 3
    @assert g(m) == 100

    @test (true)
end

true
