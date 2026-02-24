# Test for src/base/intfuncs.jl
# Based on Julia's test/intfuncs.jl
using Test

@testset "gcd/lcm" begin
    # From Julia's test/intfuncs.jl
    @test gcd(3, 5) == 1
    @test gcd(3, 15) == 3
    @test gcd(0, 15) == 15
    @test gcd(15, 0) == 15
    @test gcd(0, 0) == 0

    @test gcd(-12, 8) == 4
    @test gcd(12, -8) == 4
    @test gcd(-12, -8) == 4

    @test lcm(2, 3) == 6
    @test lcm(3, 2) == 6
    @test lcm(4, 6) == 12
    @test lcm(6, 4) == 12
    @test lcm(3, 0) == 0
    @test lcm(0, 3) == 0
    @test lcm(0, 0) == 0
end

@testset "factorial" begin
    @test factorial(0) == 1
    @test factorial(1) == 1
    @test factorial(2) == 2
    @test factorial(3) == 6
    @test factorial(4) == 24
    @test factorial(5) == 120
    @test factorial(10) == 3628800
end

@testset "isqrt" begin
    @test isqrt(0) == 0
    @test isqrt(1) == 1
    @test isqrt(4) == 2
    @test isqrt(9) == 3
    @test isqrt(16) == 4
    @test isqrt(17) == 4
    @test isqrt(24) == 4
    @test isqrt(25) == 5
end

@testset "powermod" begin
    # 2^10 mod 1000 = 1024 mod 1000 = 24
    @test powermod(2, 10, 1000) == 24
    # 3^4 mod 5 = 81 mod 5 = 1
    @test powermod(3, 4, 5) == 1
    # 2^0 mod 7 = 1
    @test powermod(2, 0, 7) == 1
end

println("test_intfuncs.jl: All tests passed!")
