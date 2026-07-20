# Regression for Issue #9342:
#
# 1. `sqrt(Complex{Int})` must not be a fatal, span-less compile-time error.
#    Previously *defining* an unreachable function whose body was
#    `sqrt(-1 + 0im)` aborted the whole program with
#    `Compilation error: Msg("Complex sqrt should use Pure Julia dispatch - ...")`.
#    Now the integer/rational Complex sqrt promotes to Complex{Float64}
#    (`sqrt(z::Complex{<:Union{Integer,Rational}}) = sqrt(float(z))`, spelled as
#    an explicit Complex{Float64} constructor so the inner sqrt statically
#    routes), and the compiler's static-routing failure falls back to that
#    promotion instead of aborting.
#
# 2. `abs(::Complex{Float16/Float32})` must preserve the component float width
#    (via `hypot`), not widen to Float64.

using Test

# An unreachable definition referencing sqrt(Complex{Int}) must not abort
# compilation of the whole program.
f() = sqrt(-1 + 0im)  # never called

@testset "Complex sqrt(Int) + abs width (Issue #9342)" begin
    # sqrt of an integer-component Complex promotes to ComplexF64.
    s = sqrt(-1 + 0im)
    @test (real(s)) == 0.0
    @test (imag(s)) == 1.0
    @test s isa Complex{Float64}

    # sqrt(3 + 4im) == 2.0 + 1.0im (matches official Julia).
    s2 = sqrt(3 + 4im)
    @test (real(s2)) == 2.0
    @test (imag(s2)) == 1.0

    # A variable-typed Complex{Int} still routes correctly.
    z = 3 + 4im
    @test (real(sqrt(z))) == 2.0
    @test (imag(sqrt(z))) == 1.0

    # Complex{Float64} sqrt is unchanged.
    @test (real(sqrt(2.0 + 0.0im))) == 1.4142135623730951

    # abs preserves the component float width.
    @test (typeof(abs(3.0f0 + 4.0f0im))) == Float32
    @test (abs(3.0f0 + 4.0f0im)) == 5.0f0
    @test (typeof(abs(Complex{Float16}(3, 4)))) == Float16

    # abs of an integer-component Complex is Float64 (as in Julia).
    @test (typeof(abs(3 + 4im))) == Float64
    @test (abs(3 + 4im)) == 5.0
end

true
