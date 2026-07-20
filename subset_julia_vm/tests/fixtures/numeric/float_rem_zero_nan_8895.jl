using Test

@testset "float remainder with zero denominator returns NaN (Issue #8895)" begin
    r = rem(Float64(2.5), Int64(0))
    @test typeof(r) == Float64
    @test isnan(r)

    r = mod(Float64(2.5), Int64(0))
    @test typeof(r) == Float64
    @test isnan(r)

    r = Float64(2.5) % Int64(0)
    @test typeof(r) == Float64
    @test isnan(r)

    r = rem(Int64(3), Float64(0.0))
    @test typeof(r) == Float64
    @test isnan(r)

    r = mod(Int64(3), Float64(0.0))
    @test typeof(r) == Float64
    @test isnan(r)

    r32 = rem(Float32(2.5), Int64(0))
    @test typeof(r32) == Float32
    @test isnan(r32)

    r16 = rem(Float16(2.5), Int64(0))
    @test typeof(r16) == Float16
    @test isnan(r16)

    @test isnan(rem(Float64(2.5), false))
    @test_throws DivideError rem(Int64(3), Int64(0))
end

true
