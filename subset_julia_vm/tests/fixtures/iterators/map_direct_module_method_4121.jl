using Test

double(x) = x * 2
add_pair(x, y) = x + y
boom(x) = error("direct Iterators.map should stay lazy")
boom_pair(x, y) = error("direct Iterators.map vararg should stay lazy")

forward_generator(f, arg, args...) = Base.Generator(f, arg, args...)

@testset "direct Iterators.map module method dispatch (Issue #4121)" begin
    g = Iterators.map(double, [1, 2, 3])
    @test g isa Base.Generator
    @test collect(g) == [2, 4, 6]

    base_g = Base.Iterators.map(double, [1, 2, 3])
    @test base_g isa Base.Generator
    @test collect(base_g) == [2, 4, 6]

    pair = Iterators.map(add_pair, [1, 2, 3], [10, 20])
    @test pair isa Base.Generator
    @test collect(pair) == [11, 22]

    forwarded_pair = forward_generator(add_pair, [1, 2, 3], [10, 20])
    @test forwarded_pair isa Base.Generator
    @test collect(forwarded_pair) == [11, 22]

    lazy_error = Iterators.map(boom, [1])
    @test lazy_error isa Base.Generator
    @test_throws ErrorException iterate(lazy_error)

    lazy_pair_error = Iterators.map(boom_pair, [1], [10])
    @test lazy_pair_error isa Base.Generator
    @test_throws ErrorException iterate(lazy_pair_error)
end

true
