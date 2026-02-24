# findmax(f, A) and findmin(f, A) - function argument forms (Issue #1998)

using Test

@testset "findmax and findmin with function argument" begin
    # findmax(f, arr) - returns (max f(x), index)
    @test findmax(abs, [-3, 1, 2]) == (3, 1)
    @test findmax(x -> -x, [1, 2, 3]) == (-1, 1)
    @test findmax(x -> x * x, [-2, 1, 3]) == (9, 3)

    # findmax(f, arr) - single element
    @test findmax(abs, [5]) == (5, 1)

    # findmax(f, arr) - first max wins (tie-breaking)
    @test findmax(abs, [3, -3, 1]) == (3, 1)

    # findmin(f, arr) - returns (min f(x), index)
    @test findmin(abs, [-3, 1, 2]) == (1, 2)
    @test findmin(x -> -x, [1, 2, 3]) == (-3, 3)
    @test findmin(x -> x * x, [3, -1, 2]) == (1, 2)

    # findmin(f, arr) - single element
    @test findmin(abs, [-7]) == (7, 1)

    # findmin(f, arr) - first min wins (tie-breaking)
    @test findmin(abs, [1, -1, 2]) == (1, 1)
end

true
