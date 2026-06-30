using Test

double(x) = x * 2
add_pair(x, y) = x + y
boom(x) = error("lazy generator boom")
boom_pair(x, y) = error("lazy vararg generator boom")

@testset "Base.Generator iterate applies function" begin
    g = Base.Generator(double, [1, 2])
    first = iterate(g)
    @test first[1] == 2
    @test typeof(first[1]) === Int64
    second = iterate(g, first[2])
    @test second[1] == 4
    @test typeof(second[1]) === Int64
    @test iterate(g, second[2]) === nothing

    pair = Base.Generator(add_pair, [1, 2, 3], [10, 20])
    pair_first = iterate(pair)
    @test pair_first[1] == 11
    pair_second = iterate(pair, pair_first[2])
    @test pair_second[1] == 22
    @test iterate(pair, pair_second[2]) === nothing

    @test collect(g) == [2, 4]
    @test collect(pair) == [11, 22]

    lazy_error = Base.Generator(boom, [1])
    @test_throws ErrorException iterate(lazy_error)

    lazy_pair_error = Base.Generator(boom_pair, [1], [10])
    @test_throws ErrorException iterate(lazy_pair_error)
end

true
