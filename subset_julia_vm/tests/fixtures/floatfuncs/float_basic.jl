using Test

@testset "float arithmetic" begin
    @test 2.0 + 3.0 == 5.0
    @test 10.0 - 3.0 == 7.0
    @test 2.0 * 3.5 == 7.0
    @test 10.0 / 2.0 == 5.0
    @test 2.0 ^ 3.0 == 8.0
end

@testset "float comparisons" begin
    @test 3.5 > 2.0
    @test 2.0 <= 2.0
    @test 3.14 == 3.14
    @test 3.14 != 2.71
end

@testset "float abs and signbit" begin
    @test abs(3.14) == 3.14
    @test abs(-3.14) == 3.14
    @test signbit(-1.0) == true
    @test signbit(1.0) == false
end

true
