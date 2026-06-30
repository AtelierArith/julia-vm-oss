using Test

# Issue #5684: maximum/minimum accept the `init` keyword, which seeds the
# reduction (and makes the result defined for an empty collection). sjulia
# rejected `init` ("unsupported keyword argument").

@testset "maximum/minimum with init keyword (Issue #5684)" begin
    @test maximum([3, 1, 2]; init=-99) == 3
    @test maximum(Int[]; init=-99) == -99      # empty -> init
    @test minimum([3, 1, 2]; init=99) == 1
    @test minimum(Int[]; init=99) == 99        # empty -> init
    @test maximum([1, 2]; init=10) == 10        # init dominates
    @test minimum([5, 6]; init=1) == 1

    # f-form with init.
    @test maximum(abs, [-5, 3]; init=0) == 5
    @test minimum(abs, [-5, 3]; init=100) == 3
    @test maximum(abs, Int[]; init=0) == 0
end

@testset "maximum/minimum without init are unchanged (Issue #5684)" begin
    @test maximum([3, 1, 2]) == 3
    @test minimum([3, 1, 2]) == 1
    @test maximum(abs, [-5, 3]) == 5
    @test minimum(abs, [-5, 3]) == 3
end

true
