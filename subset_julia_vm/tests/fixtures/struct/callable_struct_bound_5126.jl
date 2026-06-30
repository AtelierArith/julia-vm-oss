# Bound callable struct / functor dispatch: `(obj::T)(args...)` (Issue #5126)
#
# A struct instance becomes callable by defining a method on its type. The
# instance binds to the named `self` parameter, so the body can read fields.
# Both the full form `function (p::Poly)(x) ... end` and the short form
# `(p::Poly)(x) = ...` are supported, including parametric functors with a
# `where` clause.

using Test

# ---- Polynomial functor (full form, Horner's method) ----
struct Poly
    coeffs::Vector{Int}
end
function (p::Poly)(x)
    s = 0
    for c in p.coeffs
        s = s * x + c
    end
    s
end

# ---- Polynomial functor (short form) ----
struct PolyShort
    coeffs::Vector{Int}
end
(p::PolyShort)(x) = begin
    s = 0
    for c in p.coeffs
        s = s * x + c
    end
    s
end

# ---- Accumulator functor (mutable, full form) ----
mutable struct Accumulator
    total::Int
end
function (a::Accumulator)(x)
    a.total += x
    a.total
end

# ---- Affine functor with two arguments ----
struct Affine
    a::Int
    b::Int
end
function (f::Affine)(x, y)
    f.a * x + f.b * y
end

# ---- Parametric functor with where clause ----
struct Scaler{T}
    factor::T
end
function (s::Scaler{T})(x) where {T}
    s.factor * x
end

# ---- Varargs functor ----
struct Summer end
function (::Summer)(xs...)
    s = 0
    for x in xs
        s += x
    end
    s
end

@testset "Bound callable struct dispatch (Issue #5126)" begin
    # Polynomial functor: 1*x^2 + 2*x + 3 at x = 10 -> 123
    p = Poly([1, 2, 3])
    @test p(10) == 123
    @test p(0) == 3
    @test p(1) == 6

    # Short form behaves identically
    ps = PolyShort([1, 2, 3])
    @test ps(10) == 123
    @test ps(2) == 11

    # Accumulator threads state through the instance
    acc = Accumulator(0)
    @test acc(5) == 5
    @test acc(10) == 15
    @test acc(3) == 18

    # Affine functor: 2*x + 3*y
    aff = Affine(2, 3)
    @test aff(10, 100) == 320
    @test aff(1, 1) == 5

    # Parametric functor dispatches on the base type, reading `factor`
    si = Scaler(3)
    @test si(10) == 30
    sf = Scaler(2.5)
    @test sf(4) == 10.0

    # Anonymous varargs functor still works alongside bound ones
    @test Summer()(1, 2, 3, 4) == 10
    @test Summer()() == 0
end

true
