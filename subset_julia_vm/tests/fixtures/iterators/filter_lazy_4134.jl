# Iterators.filter laziness
#
# Julia upstream implements Iterators.filter as Base.Iterators.Filter in
# julia/base/iterators.jl. Construction must not call the predicate; predicate
# calls happen only while iterating or collecting the returned iterator.

using Test

function iterators_filter_is_even_4134(x)
    x % 2 == 0
end

function iterators_filter_boom_4134(x)
    error("Iterators.filter predicate should be lazy")
end

@testset "Iterators.filter is lazy" begin
    lazy = Iterators.filter(iterators_filter_is_even_4134, [1, 2, 3, 4])
    @test !(lazy isa Vector)
    @test collect(lazy) == [2, 4]

    base_lazy = Base.Iterators.filter(iterators_filter_is_even_4134, [1, 2, 3, 4])
    @test !(base_lazy isa Vector)
    @test collect(base_lazy) == [2, 4]

    lazy_error = Iterators.filter(iterators_filter_boom_4134, [1])
    @test !(lazy_error isa Vector)
    @test_throws ErrorException iterate(lazy_error)
end

true
