using Test

@testset "floor function" begin
    @test floor(3.7) == 3.0
    @test floor(-3.2) == -4.0
    @test floor(Int, 3.7) == 3
    @test floor(Int, -3.2) == -4
end

@testset "ceil function" begin
    @test ceil(3.2) == 4.0
    @test ceil(-3.7) == -3.0
    @test ceil(Int, 3.2) == 4
    @test ceil(Int, -3.7) == -3
end

@testset "round function" begin
    @test round(3.5) == 4.0
    @test round(4.5) == 4.0
    @test round(Int, 3.7) == 4
    @test round(Int, -3.2) == -3
end

@testset "trunc function" begin
    @test trunc(3.9) == 3.0
    @test trunc(-3.9) == -3.0
    @test trunc(Int, 3.9) == 3
    @test trunc(Int, -3.9) == -3
end

true
