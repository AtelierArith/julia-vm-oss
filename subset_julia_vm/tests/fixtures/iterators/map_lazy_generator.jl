using Test

double(x) = x * 2
add_pair(x, y) = x + y
boom(x) = error("iterators map should be lazy")
boom_pair(x, y) = error("iterators map vararg should be lazy")

@testset "Iterators.map lazy Base.Generator parity (Issue #4115)" begin
    g = Iterators.map(double, [1, 2, 3])
    @test g isa Base.Generator
    @test !(g isa Vector)
    @test collect(g) == [2, 4, 6]

    base_g = Base.Iterators.map(double, [1, 2, 3])
    @test base_g isa Base.Generator
    @test collect(base_g) == [2, 4, 6]

    first = iterate(g)
    @test first[1] == 2
    second = iterate(g, first[2])
    @test second[1] == 4

    pair = Iterators.map(add_pair, [1, 2, 3], [10, 20])
    @test pair isa Base.Generator
    @test !(pair isa Vector)
    @test collect(pair) == [11, 22]

    pair_first = iterate(pair)
    @test pair_first[1] == 11
    pair_second = iterate(pair, pair_first[2])
    @test pair_second[1] == 22
    @test iterate(pair, pair_second[2]) === nothing

    lazy_error = Iterators.map(boom, [1])
    @test lazy_error isa Base.Generator
    @test_throws ErrorException iterate(lazy_error)

    lazy_pair_error = Iterators.map(boom_pair, [1], [10])
    @test lazy_pair_error isa Base.Generator
    @test_throws ErrorException iterate(lazy_pair_error)
end

true
