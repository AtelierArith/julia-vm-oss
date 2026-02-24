# Test sign() function (Issue #1861)

using Test

@testset "sign positive" begin
    @test sign(5) == 1
    @test sign(3.14) == 1
    @test sign(100) == 1
end

@testset "sign negative" begin
    @test sign(-5) == -1
    @test sign(-3.14) == -1
    @test sign(-100) == -1
end

@testset "sign zero" begin
    @test sign(0) == 0
    @test sign(0.0) == 0
end

true
