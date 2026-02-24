# Test gcd() and lcm() for Int64 (Issue #1865)

using Test

@testset "gcd basic" begin
    @test gcd(12, 8) == 4
    @test gcd(48, 18) == 6
    @test gcd(7, 13) == 1
    @test gcd(0, 5) == 5
    @test gcd(5, 0) == 5
    @test gcd(0, 0) == 0
end

@testset "gcd negative" begin
    @test gcd(-12, 8) == 4
    @test gcd(12, -8) == 4
    @test gcd(-12, -8) == 4
end

@testset "lcm basic" begin
    @test lcm(4, 6) == 12
    @test lcm(12, 18) == 36
    @test lcm(7, 13) == 91
    @test lcm(0, 5) == 0
    @test lcm(5, 0) == 0
end

@testset "gcd lcm relationship" begin
    a = 12
    b = 18
    @test gcd(a, b) * lcm(a, b) == abs(a * b)
end

true
