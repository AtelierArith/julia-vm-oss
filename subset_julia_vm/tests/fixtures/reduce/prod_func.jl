# prod(f, A) - function argument form (Issue #2000)

using Test

@testset "prod with function argument" begin
    # prod(f, arr) - product of f applied to each element
    @test prod(abs, [-2, 3, -4]) == 24
    @test prod(x -> x * x, [1, 2, 3]) == 36
    @test prod(x -> x + 1, [0, 1, 2]) == 6

    # prod(f, arr) - single element
    @test prod(abs, [-5]) == 5

    # prod(f, arr) - identity
    @test prod(x -> x, [2, 3, 4]) == 24
end

true
