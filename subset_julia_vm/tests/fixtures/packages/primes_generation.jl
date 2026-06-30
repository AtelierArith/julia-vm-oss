using Test
using Primes

@testset "Primes: primes, prime" begin
    @test primes(10) == [2, 3, 5, 7]
    @test primes(10, 20) == [11, 13, 17, 19]
    @test primes(2, 2) == [2]
    @test primes(1, 1) == []
    @test prime(1) == 2
    @test prime(4) == 7
end

true
