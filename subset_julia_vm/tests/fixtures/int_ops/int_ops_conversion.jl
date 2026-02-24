using Test

@testset "integer type conversion" begin
    @test Int64(3.0) == 3
    @test Int64(true) == 1
    @test Int64(false) == 0
end

@testset "integer count_ones and leading_zeros" begin
    @test count_ones(7) == 3
    @test count_ones(0) == 0
    @test count_ones(255) == 8
    @test leading_zeros(Int64(1)) == 63
    @test leading_zeros(Int64(0)) == 64
end

@testset "integer ndigits" begin
    @test ndigits(0) == 1
    @test ndigits(9) == 1
    @test ndigits(10) == 2
    @test ndigits(999) == 3
    @test ndigits(-42) == 2
end

@testset "integer min max" begin
    @test min(3, 5) == 3
    @test max(3, 5) == 5
    @test min(-1, 1) == -1
    @test max(-1, 1) == 1
end

true
