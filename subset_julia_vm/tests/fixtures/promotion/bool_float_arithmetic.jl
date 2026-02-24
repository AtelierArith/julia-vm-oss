# Test Bool + Float arithmetic (Issue #1634)
# Julia allows Bool to participate in float arithmetic

using Test

@testset "Bool + Float arithmetic" begin
    # Bool + Float64
    @test (true + 2.5) == 3.5
    @test (false + 2.5) == 2.5
    @test (2.5 + true) == 3.5
    @test (2.5 + false) == 2.5

    # Bool - Float64
    @test (true - 0.5) == 0.5
    @test (2.5 - true) == 1.5

    # Bool * Float64
    @test (true * 2.5) == 2.5
    @test (false * 2.5) == 0.0
    @test (2.5 * true) == 2.5

    # Bool / Float64
    @test (true / 2.0) == 0.5
    @test (2.0 / true) == 2.0

    # With variables
    f = 2.5
    t = true
    @test (f + t) == 3.5
    @test (t + f) == 3.5

    # Bool + Float32
    f32 = Float32(2.5)
    @test (true + f32) == Float32(3.5)
    @test (f32 + true) == Float32(3.5)
end

true
