# Test flipsign() function (Issue #1870)

using Test

@testset "flipsign basic" begin
    @test flipsign(1, 1) == 1
    @test flipsign(1, -1) == -1
    @test flipsign(-1, 1) == -1
    @test flipsign(-1, -1) == 1
end

@testset "flipsign float" begin
    @test flipsign(3.0, 1.0) == 3.0
    @test flipsign(3.0, -1.0) == -3.0
    @test flipsign(-3.0, 1.0) == -3.0
    @test flipsign(-3.0, -1.0) == 3.0
end

@testset "flipsign zero" begin
    @test flipsign(0, 1) == 0
    @test flipsign(0, -1) == 0
end

true
