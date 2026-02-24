# Iterators.flatmap - map then flatten
# Issue #2115

using Test

expand_pair(n) = [n, n * 10]
make_range(n) = 1:n

@testset "flatmap basic" begin
    # flatmap with array-returning function
    result = collect(flatmap(expand_pair, [1, 2, 3]))
    @test result == [1, 10, 2, 20, 3, 30]

    # flatmap with range-returning function
    result2 = collect(flatmap(make_range, [1, 2, 3]))
    @test result2 == [1, 1, 2, 1, 2, 3]

    # flatmap with lambda
    result3 = collect(flatmap(x -> [x, x + 10], [1, 2, 3]))
    @test result3 == [1, 11, 2, 12, 3, 13]

    # flatmap with single-element collections
    result4 = collect(flatmap(x -> [x * 2], [5, 10, 15]))
    @test result4 == [10, 20, 30]
end

true
