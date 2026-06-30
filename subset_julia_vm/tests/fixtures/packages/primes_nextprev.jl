using Test
using Primes

@testset "Primes: nextprime and prevprime" begin
    @test nextprime(1) == 2
    @test nextprime(2) == 2
    @test nextprime(3) == 3
    @test nextprime(4) == 5
    @test nextprime(10) == 11
    @test prevprime(3) == 2
    @test prevprime(7) == 7
    @test prevprime(10) == 7
end

true
