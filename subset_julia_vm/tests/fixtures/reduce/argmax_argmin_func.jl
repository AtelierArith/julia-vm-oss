# argmax(f, A) and argmin(f, A) - function argument forms (Issue #1998)

using Test

@testset "argmax and argmin with function argument" begin
    # argmax(f, arr) - returns element x that maximizes f(x)
    @test argmax(abs, [-3, 1, 2]) == -3
    @test argmax(x -> -x, [1, 2, 3]) == 1
    @test argmax(x -> x * x, [-2, 1, 3]) == 3

    # argmax(f, arr) - single element
    @test argmax(abs, [5]) == 5

    # argmax(f, arr) - first max wins (tie-breaking)
    @test argmax(abs, [3, -3, 1]) == 3

    # argmin(f, arr) - returns element x that minimizes f(x)
    @test argmin(abs, [-3, 1, 2]) == 1
    @test argmin(x -> -x, [1, 2, 3]) == 3
    @test argmin(x -> x * x, [3, -1, 2]) == -1

    # argmin(f, arr) - single element
    @test argmin(abs, [-7]) == -7

    # argmin(f, arr) - first min wins (tie-breaking)
    @test argmin(abs, [1, -1, 2]) == 1
end

true
