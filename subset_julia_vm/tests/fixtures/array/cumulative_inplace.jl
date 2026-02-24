# Test in-place cumulative functions: cumsum!, cumprod!, accumulate!

using Test

@testset "cumsum! basic" begin
    x = [1.0, 2.0, 3.0, 4.0]
    y = zeros(4)
    cumsum!(y, x)
    @test y[1] == 1.0
    @test y[2] == 3.0
    @test y[3] == 6.0
    @test y[4] == 10.0
end

@testset "cumsum! integer values" begin
    x = [1.0, 1.0, 1.0, 1.0, 1.0]
    y = zeros(5)
    cumsum!(y, x)
    @test y[5] == 5.0
end

@testset "cumprod! basic" begin
    x = [1.0, 2.0, 3.0, 4.0]
    y = zeros(4)
    cumprod!(y, x)
    @test y[1] == 1.0
    @test y[2] == 2.0
    @test y[3] == 6.0
    @test y[4] == 24.0
end

@testset "cumprod! with zeros" begin
    x = [2.0, 3.0, 0.0, 5.0]
    y = zeros(4)
    cumprod!(y, x)
    @test y[1] == 2.0
    @test y[2] == 6.0
    @test y[3] == 0.0
    @test y[4] == 0.0
end

@testset "accumulate! with min" begin
    x = [3.0, 1.0, 4.0, 1.0, 5.0]
    y = zeros(5)
    accumulate!(min, y, x)
    @test y[1] == 3.0
    @test y[2] == 1.0
    @test y[3] == 1.0
    @test y[4] == 1.0
    @test y[5] == 1.0
end

@testset "accumulate! with max" begin
    x = [1.0, 3.0, 2.0, 5.0, 4.0]
    y = zeros(5)
    accumulate!(max, y, x)
    @test y[1] == 1.0
    @test y[2] == 3.0
    @test y[3] == 3.0
    @test y[4] == 5.0
    @test y[5] == 5.0
end

true
