using Test
using Primes

@testset "Primes: radical and totient" begin
    @test radical(12) == 6
    @test radical(360) == 30
    @test radical(0) == 0
    @test totient(1) == 1
    @test totient(36) == 12
    @test totient(10) == 4
end

true
