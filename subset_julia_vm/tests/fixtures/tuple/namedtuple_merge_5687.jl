using Test

@testset "NamedTuple merge" begin
    @test merge((a = 1, b = 2), (b = 20, c = 3)) == (a = 1, b = 20, c = 3)
    @test merge((a = 1,), (b = 2,)) == (a = 1, b = 2)
    @test merge((x = 1, y = 2), (y = 99,)) == (x = 1, y = 99)

    left = (a = 1, b = 2)
    right = (b = 20, c = 3)
    merged = merge(left, right)
    @test merged == (a = 1, b = 20, c = 3)
    @test merged.c == 3
    @test typeof(merged) === NamedTuple{(:a, :b, :c), Tuple{Int64, Int64, Int64}}

    @test merge((a = 1, b = 2), (b = 20, c = 3), (a = 10, d = 4)) ==
        (a = 10, b = 20, c = 3, d = 4)
    @test merge((a = 1, b = 2), (;)) == (a = 1, b = 2)
    @test merge((;), (b = 2,)) == (b = 2,)
    @test NamedTuple() == (;)
    @test merge((a = 1, b = 2)) == (a = 1, b = 2)
end

true
