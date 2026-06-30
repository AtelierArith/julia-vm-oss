# Test Complex{T} parametric completeness for arbitrary T<:Real (Issue #5131)
#
# Before the fix, `Complex(x::Real, y::Real) = Complex(promote(x, y)...)`
# recursed infinitely for any element type not covered by an explicit concrete
# overload (Int32, Int16, Rational, ...). The generic two-argument and
# single-argument constructors must preserve and infer the element type T
# exactly like upstream Julia.

using Test

@testset "Complex{T} parametric completeness (Issue #5131)" begin
    # Two-argument: Int32 pair preserves Complex{Int32}
    z1 = Complex(Int32(1), Int32(2))
    @test typeof(z1) === Complex{Int32}
    @test real(z1) === Int32(1)
    @test imag(z1) === Int32(2)

    # Two-argument: Int16 pair preserves Complex{Int16}
    z2 = Complex(Int16(3), Int16(4))
    @test typeof(z2) === Complex{Int16}

    # Two-argument: Rational pair preserves Complex{Rational{Int64}}.
    # (Part values are compared with `==`; SubsetJuliaVM `===` on Rational
    # values is a separate, pre-existing limitation unrelated to #5131.)
    z3 = Complex(1 // 2, 3 // 4)
    @test typeof(z3) === Complex{Rational{Int64}}
    @test real(z3) == 1 // 2
    @test imag(z3) == 3 // 4

    # Single-argument: Int32 preserves Complex{Int32}, imaginary part zero
    z4 = Complex(Int32(5))
    @test typeof(z4) === Complex{Int32}
    @test real(z4) === Int32(5)
    @test imag(z4) === Int32(0)

    # Single-argument: Rational preserves Complex{Rational{Int64}}
    z5 = Complex(1 // 2)
    @test typeof(z5) === Complex{Rational{Int64}}

    # Mixed-type two-argument still promotes correctly
    z6 = Complex(Int32(1), 2.0)
    @test typeof(z6) === Complex{Float64}
end

true  # Test passed
