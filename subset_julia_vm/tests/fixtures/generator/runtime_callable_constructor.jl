using Test

double(x) = x * 2
add_pair(x, y) = x + y

function make_generator(f, xs)
    return Base.Generator(f, xs)
end

function make_pair_generator(f, xs, ys)
    return Base.Generator(f, xs, ys)
end

@testset "runtime callable Base.Generator constructor parity (Issue #4118)" begin
    g = make_generator(double, [1, 2, 3])
    @test g isa Base.Generator
    @test collect(g) == [2, 4, 6]

    first = iterate(g)
    @test first[1] == 2
    second = iterate(g, first[2])
    @test second[1] == 4

    pair = make_pair_generator(add_pair, [1, 2, 3], [10, 20])
    @test pair isa Base.Generator
    @test collect(pair) == [11, 22]

    pair_first = iterate(pair)
    @test pair_first[1] == 11
    pair_second = iterate(pair, pair_first[2])
    @test pair_second[1] == 22
    @test iterate(pair, pair_second[2]) === nothing
end

true
