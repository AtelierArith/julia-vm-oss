# Julia Manual: Mathematical Operations and Elementary Functions
# https://docs.julialang.org/en/v1/manual/mathematical-operations/
# Tests operators, comparison, and math functions.

using Test

@testset "Arithmetic operators" begin
    @test +3 == 3
    @test -3 == -3
    @test 2 + 3 == 5
    @test 2 - 3 == -1
    @test 2 * 3 == 6
    @test 7 / 2 == 3.5
    @test div(7, 2) == 3
    @test 7 % 2 == 1
    @test 2^3 == 8
end

@testset "Comparison operators" begin
    @test 1 == 1
    @test 1 != 2
    @test 1 < 2
    @test 2 > 1
    @test 1 <= 1
    @test 1 >= 1
    @test 1 <= 2
    @test 2 >= 1
end

@testset "Chained comparisons" begin
    @test 1 < 2 < 3
    @test 1 < 2 <= 2
    @test 1 <= 1 < 2
end

@testset "Boolean operators" begin
    @test !false == true
    @test !true == false
    @test (true && true) == true
    @test (true && false) == false
    @test (false || true) == true
    @test (false || false) == false
end

@testset "Updating operators" begin
    x = 10
    x += 5
    @test x == 15
    x -= 3
    @test x == 12
    x *= 2
    @test x == 24
    x = div(x, 4)
    @test x == 6
end

@testset "Math functions" begin
    @test abs(-5) == 5
    @test abs(5) == 5
    @test sign(-3) == -1
    @test sign(0) == 0
    @test sign(3) == 1
    @test sqrt(4.0) == 2.0
    @test min(3, 5) == 3
    @test max(3, 5) == 5
    @test clamp(7, 1, 5) == 5
    @test clamp(-1, 1, 5) == 1
    @test clamp(3, 1, 5) == 3
end

@testset "Rounding" begin
    @test round(1.5) == 2.0
    @test floor(1.9) == 1.0
    @test ceil(1.1) == 2.0
    @test trunc(1.9) == 1.0
    @test trunc(-1.9) == -1.0
end

true
