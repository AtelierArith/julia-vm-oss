# extrema(f, A) - function argument form (Issue #2000)

using Test

@testset "extrema with function argument" begin
    # extrema(f, arr) - returns (min(f(x)), max(f(x)))
    @test extrema(abs, [-3, 1, 2]) == (1, 3)
    @test extrema(x -> -x, [1, 2, 3]) == (-3, -1)
    @test extrema(x -> x * x, [-2, 1, 3]) == (1, 9)

    # extrema(f, arr) - single element
    @test extrema(abs, [5]) == (5, 5)

    # extrema(f, arr) - all same after applying f
    @test extrema(abs, [2, -2]) == (2, 2)
end

true
