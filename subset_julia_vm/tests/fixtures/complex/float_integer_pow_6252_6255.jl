using Test

# Issues #6252/#6253: Complex floating-point bases raised to integer powers
# should use integer-power semantics (`z*z` for `z^2`), not the analytic
# real-exponent path. Issue #6255 covers the ComplexF32 type-preservation checks.
# Keep the Float64/Float32 variables under distinct names until @testset-local
# slot reuse is fixed (Issue #6256).

@testset "ComplexF64 integer powers (Issues #6252, #6253)" begin
    z64 = 1.0 + 2.0im
    @test z64^0 == one(z64)
    @test z64^1 == z64
    @test z64^2 == z64 * z64
    @test z64^2 == -3.0 + 4.0im
    @test z64^-1 == inv(z64)
    @test z64^-2 == inv(z64) * inv(z64)

    i = 0.0 + 1.0im
    @test i^2 == -1.0 + 0.0im
end

@testset "ComplexF32 integer powers preserve type (Issue #6255)" begin
    z32 = ComplexF32(1, 2)
    @test typeof(z32 + z32) == ComplexF32
    @test typeof(z32 - z32) == ComplexF32
    @test typeof(z32 * z32) == ComplexF32
    @test typeof(inv(z32)) == ComplexF32
    @test typeof(z32^0) == ComplexF32
    @test typeof(z32^1) == ComplexF32
    @test typeof(z32^2) == ComplexF32
    @test typeof(z32^-1) == ComplexF32
    @test typeof(z32^-2) == ComplexF32
    @test z32^2 == z32 * z32
    @test z32^-2 == inv(z32) * inv(z32)

    w = ComplexF32(3, 4)
    @test typeof(abs2(w)) == Float32
    @test abs2(w) == Float32(25)
end

true
