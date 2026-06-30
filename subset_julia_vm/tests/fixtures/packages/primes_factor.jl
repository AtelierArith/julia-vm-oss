using Test
using Primes

@testset "Primes: factor and prodfactors" begin
    @test factor(Vector, 1) == []
    @test factor(Vector, 2) == [2]
    @test factor(Vector, 12) == [2, 2, 3]
    @test factor(Vector, 360) == [2, 2, 2, 3, 3, 5]
    @test prodfactors(factor(360)) == 360
    @test prodfactors(factor(Vector, 360)) == 360
end

true
