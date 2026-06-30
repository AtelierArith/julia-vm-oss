# Rational{T} parametric constructor completeness (Issue #5132)
#
# Explicit type-parameter constructors must:
#   - coerce the numerator/denominator to the requested element type T
#   - normalize (gcd reduction + positive denominator)
#   - support the single-argument form Rational{T}(x) == Rational{T}(x, 1)
#   - support Rational-from-Rational conversion Rational{T}(r::Rational)
#
# Established against upstream Julia 1.12.6.

using Test

@testset "Rational parametric typed constructor" begin
    # Explicit type param coerces field type and normalizes
    a = Rational{Int8}(6, 4)
    @test typeof(a) === Rational{Int8}
    @test a.num === Int8(3)
    @test a.den === Int8(2)

    # Negative denominator normalization with explicit type
    b = Rational{Int8}(6, -4)
    @test typeof(b) === Rational{Int8}
    @test b.num === Int8(-3)
    @test b.den === Int8(2)

    # Already-typed integer args, still reduces
    c = Rational{Int8}(Int8(6), Int8(4))
    @test typeof(c) === Rational{Int8}
    @test c.num === Int8(3)

    # Single-argument constructor: den = 1
    d = Rational{Int8}(3)
    @test typeof(d) === Rational{Int8}
    @test d.num === Int8(3)
    @test d.den === Int8(1)

    # Rational-from-Rational conversion constructor
    e = Rational{Int64}(Int8(3)//Int8(4))
    @test typeof(e) === Rational{Int64}
    @test e.num === Int64(3)
    @test e.den === Int64(4)

    # BigInt type param
    f = Rational{BigInt}(6, 4)
    @test typeof(f) === Rational{BigInt}
    @test f.num == big(3)
    @test f.den == big(2)

    # Int64 explicit (already worked) still correct
    g = Rational{Int64}(1, 2)
    @test typeof(g) === Rational{Int64}
    @test g.num === 1
    @test g.den === 2

    # Int32 type param
    h = Rational{Int32}(10, 4)
    @test typeof(h) === Rational{Int32}
    @test h.num === Int32(5)
    @test h.den === Int32(2)
end

true
