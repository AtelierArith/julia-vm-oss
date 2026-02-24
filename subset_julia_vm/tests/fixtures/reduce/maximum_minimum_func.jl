# maximum(f, A) and minimum(f, A) - function argument forms (Issue #2000)

using Test

@testset "maximum and minimum with function argument" begin
    # maximum(f, arr) - returns max of f applied to each element
    @test maximum(abs, [-3, 1, 2]) == 3
    @test maximum(x -> -x, [1, 2, 3]) == -1
    @test maximum(x -> x * x, [-2, 1, 3]) == 9

    # maximum(f, arr) - single element
    @test maximum(abs, [5]) == 5

    # minimum(f, arr) - returns min of f applied to each element
    @test minimum(abs, [-3, 1, 2]) == 1
    @test minimum(x -> -x, [1, 2, 3]) == -3
    @test minimum(x -> x * x, [3, -1, 2]) == 1

    # minimum(f, arr) - single element
    @test minimum(abs, [-7]) == 7
end

true
