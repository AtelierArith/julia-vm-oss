using Test

@testset "filtered generator as nested generator base" begin
    @test collect(v for v in (x for x in 1:5 if x > 2)) == [3, 4, 5]
    @test map(x -> x + 1, (x for x in 1:5 if x > 2)) == [4, 5, 6]
    @test sum(v for v in (x^2 for x in 1:5 if isodd(x))) == 35

    threshold = 2
    @test collect(v * 10 for v in (x for x in 1:5 if x > threshold)) == [30, 40, 50]
end

true
