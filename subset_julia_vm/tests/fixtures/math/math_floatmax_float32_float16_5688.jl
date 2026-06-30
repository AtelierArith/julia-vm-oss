using Test

# Issue #5688: floatmax/floatmin for Float32 and Float16 (only Float64 existed).

@testset "floatmax/floatmin for Float32 and Float16 (Issue #5688)" begin
    @test floatmax(Float32) === floatmax(Float32)
    @test floatmax(Float32) == 3.4028235f38
    @test floatmin(Float32) == 1.1754944f-38
    @test typeof(floatmax(Float32)) === Float32
    @test typeof(floatmin(Float32)) === Float32

    @test floatmax(Float16) == Float16(65504.0)
    @test floatmin(Float16) == Float16(6.103515625e-5)
    @test typeof(floatmax(Float16)) === Float16

    @test floatmin(Float32) > 0.0f0
    @test floatmax(Float32) > floatmin(Float32)

    # Float64 forms unchanged.
    @test floatmax(Float64) == 1.7976931348623157e308
    @test floatmin(Float64) == 2.2250738585072014e-308
end

true
