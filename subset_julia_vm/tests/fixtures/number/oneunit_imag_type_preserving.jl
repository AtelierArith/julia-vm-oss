# Regression: oneunit(x) and imag(x) must be TYPE-PRESERVING (Issue #5039)
#
# Before the fix, the untyped fallbacks `oneunit(x) = 1` and `imag(x) = 0.0`
# in base/number.jl returned the wrong type for every numeric input:
#   oneunit(3.0) -> 1::Int64    (upstream: 1.0::Float64)
#   imag(3)      -> 0.0::Float64 (upstream: 0::Int64)
# Use `===` / typeof to catch the TYPE, not just the value (1 == 1.0 is true).

using Test

@testset "oneunit type-preserving" begin
    # Value + type must both match upstream.
    @test oneunit(3.0) === 1.0
    @test oneunit(3) === 1
    @test oneunit(Int8(5)) === Int8(1)
    @test oneunit(Int16(5)) === Int16(1)
    @test oneunit(Int32(5)) === Int32(1)
    @test oneunit(2.0f0) === 1.0f0
    @test typeof(oneunit(3.0)) === Float64
    @test typeof(oneunit(3)) === Int64

    # Type-argument form: oneunit(T) returns a value of type T.
    @test oneunit(Int64) === 1
    @test oneunit(Float64) === 1.0
    @test oneunit(Int8) === Int8(1)
    @test oneunit(Float32) === 1.0f0
end

@testset "imag type-preserving" begin
    # imag of a real returns a same-type zero (upstream imag(x::Real) = zero(x)).
    @test imag(3) === 0
    @test imag(Int8(3)) === Int8(0)
    @test imag(Int16(3)) === Int16(0)
    @test imag(Int32(3)) === Int32(0)
    @test imag(3.0) === 0.0
    @test imag(2.0f0) === 0.0f0
    @test typeof(imag(3)) === Int64
    @test typeof(imag(3.0)) === Float64
end

@testset "imag still correct for Complex" begin
    z = Complex(3, 4)
    @test imag(z) === 4
    @test real(z) === 3
    w = Complex(1.0, 2.0)
    @test imag(w) === 2.0
    @test real(w) === 1.0
end

@testset "real/conj/isreal unchanged" begin
    @test real(3) === 3
    @test real(3.0) === 3.0
    @test conj(3) === 3
    @test conj(3.0) === 3.0
    @test isreal(3) === true
    @test isreal(3.0) === true
end

true
