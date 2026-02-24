using Test

# Test that math intrinsics preserve Float32/Float16 input types (Issue #2202)
# sqrt, floor, ceil, trunc, abs, copysign should return the same float type as input.

@testset "Math intrinsics type preservation" begin
    # sqrt preserves type
    @test typeof(sqrt(Float64(4.0))) == Float64
    @test typeof(sqrt(Float32(4.0))) == Float32
    @test sqrt(Float32(4.0)) == Float32(2.0)
    @test sqrt(Float64(9.0)) == 3.0

    # floor preserves type
    @test typeof(floor(Float64(1.7))) == Float64
    @test typeof(floor(Float32(1.7))) == Float32
    @test floor(Float32(1.7)) == Float32(1.0)

    # ceil preserves type
    @test typeof(ceil(Float64(1.3))) == Float64
    @test typeof(ceil(Float32(1.3))) == Float32
    @test ceil(Float32(1.3)) == Float32(2.0)

    # trunc preserves type
    @test typeof(trunc(Float64(1.9))) == Float64
    @test typeof(trunc(Float32(1.9))) == Float32
    @test trunc(Float32(1.9)) == Float32(1.0)

    # abs preserves type
    @test typeof(abs(Float64(-1.5))) == Float64
    @test typeof(abs(Float32(-1.5))) == Float32
    @test abs(Float32(-1.5)) == Float32(1.5)

    # copysign preserves type
    @test typeof(copysign(Float64(1.0), Float64(-1.0))) == Float64
    @test typeof(copysign(Float32(1.0), Float32(-1.0))) == Float32
    @test copysign(Float32(1.0), Float32(-1.0)) == Float32(-1.0)
end

true
