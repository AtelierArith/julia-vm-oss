# Test in-place reduction functions: sum!, prod!, maximum!, minimum!

using Test

@testset "sum! reduce along dim 2 (vector)" begin
    A = [1.0 2.0; 3.0 4.0]
    r = zeros(2)
    sum!(r, A)
    @test r[1] == 3.0  # 1+2
    @test r[2] == 7.0  # 3+4
end

@testset "sum! reduce along dim 1 (row matrix)" begin
    A = [1.0 2.0; 3.0 4.0]
    r = zeros(1, 2)
    sum!(r, A)
    @test r[1, 1] == 4.0  # 1+3
    @test r[1, 2] == 6.0  # 2+4
end

@testset "sum! reduce along dim 2 (column matrix)" begin
    A = [1.0 2.0; 3.0 4.0]
    r = zeros(2, 1)
    sum!(r, A)
    @test r[1, 1] == 3.0  # 1+2
    @test r[2, 1] == 7.0  # 3+4
end

@testset "prod! reduce along dim 2 (vector)" begin
    A = [1.0 2.0; 3.0 4.0]
    r = zeros(2)
    prod!(r, A)
    @test r[1] == 2.0   # 1*2
    @test r[2] == 12.0  # 3*4
end

@testset "prod! reduce along dim 1 (row matrix)" begin
    A = [1.0 2.0; 3.0 4.0]
    r = zeros(1, 2)
    prod!(r, A)
    @test r[1, 1] == 3.0  # 1*3
    @test r[1, 2] == 8.0  # 2*4
end

@testset "maximum! reduce along dim 2 (vector)" begin
    A = [1.0 5.0; 3.0 2.0]
    r = zeros(2)
    maximum!(r, A)
    @test r[1] == 5.0  # max(1, 5)
    @test r[2] == 3.0  # max(3, 2)
end

@testset "maximum! reduce along dim 1 (row matrix)" begin
    A = [1.0 5.0; 3.0 2.0]
    r = zeros(1, 2)
    maximum!(r, A)
    @test r[1, 1] == 3.0  # max(1, 3)
    @test r[1, 2] == 5.0  # max(5, 2)
end

@testset "minimum! reduce along dim 2 (vector)" begin
    A = [1.0 5.0; 3.0 2.0]
    r = zeros(2)
    minimum!(r, A)
    @test r[1] == 1.0  # min(1, 5)
    @test r[2] == 2.0  # min(3, 2)
end

@testset "minimum! reduce along dim 1 (row matrix)" begin
    A = [1.0 5.0; 3.0 2.0]
    r = zeros(1, 2)
    minimum!(r, A)
    @test r[1, 1] == 1.0  # min(1, 3)
    @test r[1, 2] == 2.0  # min(5, 2)
end

true
