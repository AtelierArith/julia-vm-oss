using Test

# Issue #5690: nextfloat/prevfloat must preserve the float width (Float32/Float16)
# and overflow to the per-width Inf. They previously coerced the argument to Float64
# (returning Float64 and never overflowing a narrow type).

@testset "nextfloat/prevfloat width preservation (Issue #5690)" begin
    # Width preserved.
    @test nextfloat(1.0f0) isa Float32
    @test prevfloat(1.0f0) isa Float32
    @test nextfloat(Float16(1.0)) isa Float16
    @test prevfloat(Float16(1.0)) isa Float16
    @test nextfloat(1.0) isa Float64

    # Values match upstream precision.
    @test nextfloat(1.0f0) == 1.0000001f0
    @test nextfloat(Float16(1.0)) == Float16(1.0009766)
    @test prevfloat(nextfloat(3.14f0)) == 3.14f0
    @test prevfloat(nextfloat(Float16(2.5))) == Float16(2.5)

    # Per-width overflow to Inf.
    @test nextfloat(floatmax(Float32)) == Inf32
    @test nextfloat(floatmax(Float16)) == Inf16
    @test prevfloat(-floatmax(Float32)) == -Inf32
    @test nextfloat(floatmax(Float64)) == Inf

    # Edge cases.
    @test nextfloat(0.0f0) isa Float32
    @test nextfloat(Inf32) == Inf32
    @test prevfloat(-Inf32) == -Inf32
    @test isnan(nextfloat(NaN32))

    # Via a variable whose runtime type is Float32.
    y = floatmax(Float32)
    @test nextfloat(y) == Inf32
end

true
