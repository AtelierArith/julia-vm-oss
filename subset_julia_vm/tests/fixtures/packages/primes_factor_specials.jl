using Test
using Primes

@testset "Primes: factor special cases" begin
    @test factor(Vector, 0) == []
    @test factor(Vector, -12) == [2, 2, 3]
    s12 = factor(Set, 12)
    @test length(s12) == 2
    @test issubset(Set([2, 3]), s12)
    s360 = factor(Set, 360)
    @test length(s360) == 3
    @test issubset(Set([2, 3, 5]), s360)
end

true
