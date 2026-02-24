# Identity lambda x -> x regression test (Issue #2001)

using Test

@testset "identity lambda" begin
    # Basic identity lambda
    f = x -> x
    @test f(42) == 42
    @test f(3.14) == 3.14

    # Identity lambda in HOF context
    @test map(x -> x, [1, 2, 3]) == [1, 2, 3]
    @test filter(x -> x > 2, [1, 2, 3, 4]) == [3, 4]

    # Identity lambda with findmax/findmin
    @test findmax(x -> x, [1, 5, 3]) == (5, 2)
    @test findmin(x -> x, [1, 5, 3]) == (1, 1)

    # Identity lambda with maximum/minimum
    @test maximum(x -> x, [1, 5, 3]) == 5
    @test minimum(x -> x, [1, 5, 3]) == 1
end

true
