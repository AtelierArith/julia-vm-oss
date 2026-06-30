using Test
using Iterators

double(x) = x * 2
add_pair(x, y) = x + y

function mapped(f, xs)
    return Iterators.map(f, xs)
end

function mapped_base(f, xs)
    return Base.Iterators.map(f, xs)
end

function mapped_pair(f, xs, ys)
    return Iterators.map(f, xs, ys)
end

@testset "function-valued Iterators.map runtime callable parity (Issue #4118)" begin
    m = Iterators.map
    g = m(double, [1, 2, 3])
    @test g isa Base.Generator
    @test collect(g) == [2, 4, 6]

    base_m = Base.Iterators.map
    base_g = base_m(double, [1, 2, 3])
    @test base_g isa Base.Generator
    @test collect(base_g) == [2, 4, 6]

    runtime_g = mapped(double, [1, 2, 3])
    @test runtime_g isa Base.Generator
    @test collect(runtime_g) == [2, 4, 6]

    runtime_base_g = mapped_base(double, [1, 2, 3])
    @test runtime_base_g isa Base.Generator
    @test collect(runtime_base_g) == [2, 4, 6]

    pair_g = mapped_pair(add_pair, [1, 2, 3], [10, 20])
    @test pair_g isa Base.Generator
    @test collect(pair_g) == [11, 22]
end

true
