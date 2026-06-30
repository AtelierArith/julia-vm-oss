using Test
using Iterators

double(x) = x * 2
is_even(x) = x % 2 == 0

@testset "module function values preserve Iterators identity (Issue #4119)" begin
    m = Iterators.map
    g = m(double, [1, 2, 3])
    @test g isa Base.Generator
    @test collect(g) == [2, 4, 6]

    base_m = Base.Iterators.map
    base_g = base_m(double, [1, 2, 3])
    @test base_g isa Base.Generator
    @test collect(base_g) == [2, 4, 6]

    filter_fn = Iterators.filter
    @test collect(filter_fn(is_even, [1, 2, 3, 4])) == [2, 4]

    base_filter_fn = Base.Iterators.filter
    @test collect(base_filter_fn(is_even, [1, 2, 3, 4])) == [2, 4]

    take_fn = Iterators.take
    @test collect(take_fn([1, 2, 3, 4], 2)) == [1, 2]
end

true
