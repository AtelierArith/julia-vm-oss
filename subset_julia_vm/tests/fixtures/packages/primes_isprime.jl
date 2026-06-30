using Test
using Primes

@testset "Primes: isprime basic cases" begin
    @test isprime(-1) == false
    @test isprime(0) == false
    @test isprime(1) == false
    @test isprime(2) == true
    @test isprime(3) == true
    @test isprime(4) == false
    @test isprime(97) == true
    @test isprime(100) == false
end

true
