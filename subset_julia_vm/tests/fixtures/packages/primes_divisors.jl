using Test
using Primes

@testset "Primes: divisors" begin
    @test divisors(1) == [1]
    @test divisors(6) == [1, 2, 3, 6]
    @test divisors(12) == [1, 2, 3, 4, 6, 12]
    @test divisors(0) == []
    @test divisors(-6) == [1, 2, 3, 6]
end

true
