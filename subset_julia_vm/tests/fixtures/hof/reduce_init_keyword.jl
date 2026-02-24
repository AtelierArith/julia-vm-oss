# reduce/foldl/foldr with init keyword argument (Issue #2077)
# Tests that init can be passed as keyword argument, matching Julia's standard API.

using Test

@testset "reduce/foldl/foldr init keyword" begin
    # reduce with init keyword
    @test reduce(+, [1, 2, 3]; init=10) == 16
    @test reduce(*, [1, 2, 3]; init=10) == 60

    # reduce with init keyword on empty-like cases
    @test reduce(+, [5]; init=100) == 105

    # foldl with init keyword (left-fold)
    @test foldl(-, [1, 2, 3]; init=10) == 4  # ((10 - 1) - 2) - 3

    # foldr with init keyword (right-fold)
    @test foldr(-, [1, 2, 3]; init=10) == -8  # 1 - (2 - (3 - 10))

    # mapreduce with init keyword
    @test mapreduce(x -> x^2, +, [1, 2, 3]; init=0) == 14  # 0 + 1 + 4 + 9
    @test mapreduce(x -> x^2, +, [1, 2, 3]; init=100) == 114

    # mapfoldl with init keyword
    @test mapfoldl(x -> x * 2, +, [1, 2, 3]; init=0) == 12  # 0 + 2 + 4 + 6

    # mapfoldr with init keyword
    @test mapfoldr(x -> x * 2, +, [1, 2, 3]; init=0) == 12  # 6 + 4 + 2 + 0
end

true
