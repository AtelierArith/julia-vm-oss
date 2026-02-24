# Prevention test for Issue #1673 (related to Bug #1665)
#
# When filter() returns a result with compile-time type Any,
# method dispatch must select the generic map(f::Function, A) method
# over scalar methods like map(f::Function, x::Int64).
# This test ensures HOF chains with unknown intermediate types work correctly.

using Test

double(x) = x * 2
triple(x) = x * 3
ispositive(x) = x > 0
iseven_fn(x) = x % 2 == 0

@testset "HOF chains with unknown types" begin
    # map on filter result (compile-time type: Any)
    filtered = filter(ispositive, [-1, 0, 1, 2])
    result = map(double, filtered)
    @test result == [2, 4]

    # Nested HOF chains: map(f, map(g, filter(h, arr)))
    result2 = map(triple, map(double, filter(ispositive, [1, 2, 3])))
    @test result2 == [6, 12, 18]

    # filter then filter then map
    data = [-3, -2, -1, 0, 1, 2, 3, 4]
    step1 = filter(ispositive, data)
    step2 = filter(iseven_fn, step1)
    step3 = map(double, step2)
    @test step3 == [4, 8]

    # map then filter
    doubled_all = map(double, [1, 2, 3, 4, 5])
    big_ones = filter(x -> x > 5, doubled_all)
    @test big_ones == [6, 8, 10]
end

true
