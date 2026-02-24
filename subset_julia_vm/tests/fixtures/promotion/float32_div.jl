# Test Float32 div() (integer division) mixed-type dispatch (Issue #1849)

using Test

@testset "div(Float32, Int64)" begin
    x = Float32(7.0)
    y = 2
    result = div(x, y)
    @test result == Float32(3.0)
end

@testset "div(Int64, Float32)" begin
    x = 7
    y = Float32(2.0)
    result = div(x, y)
    @test result == Float32(3.0)
end

@testset "div(Float32, Float64)" begin
    x = Float32(7.0)
    y = 2.0
    result = div(x, y)
    @test result == 3.0
end

@testset "div(Float32, Float32)" begin
    x = Float32(10.0)
    y = Float32(3.0)
    result = div(x, y)
    @test result == Float32(3.0)
end

true
