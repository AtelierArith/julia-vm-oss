# Meta.parse Float32 literals evaluate as Float32 values.

using Test

@testset "Meta.parse Float32 literal eval" begin
    expr = Meta.parse("1.0f0")
    @test typeof(expr) === Float32
    @test expr === 1.0f0

    value = eval(expr)
    @test value === 1.0f0
    @test typeof(value) === Float32

    @test eval(Meta.parse("1f0")) === 1.0f0
    @test typeof(eval(Meta.parse("1f0"))) === Float32
    @test eval(Meta.parse("1_2.5f-1")) === 1.25f0
    @test typeof(eval(Meta.parse("1_2.5f-1"))) === Float32
end

true
