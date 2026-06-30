using Test

# Issue #5761: sum/prod must accept the `init` keyword (like reduce/foldl/
# mapreduce/maximum/minimum). `init` seeds the accumulator.

@testset "sum/prod init keyword (Issue #5761)" begin
    # init seeds the accumulator
    @test sum([1, 2, 3]; init=100) == 106
    @test prod([1, 2, 3]; init=10) == 60
    @test sum([10, 20, 30]; init=5) == 65

    # init on an empty collection returns init
    @test sum(Int[]; init=42) == 42
    @test prod(Int[]; init=7) == 7

    # Float accumulator
    @test sum([1.0, 2.0]; init=0.5) == 3.5
    @test prod([2.0, 3.0]; init=2.0) == 12.0

    # init passed without the semicolon also works
    @test sum([1, 2, 3], init=0) == 6

    # Without init, behavior is unchanged
    @test sum([1, 2, 3]) == 6
    @test prod([1, 2, 3, 4]) == 24
end

true
