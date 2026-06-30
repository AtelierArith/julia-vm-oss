using Test

# Issue #5681 (partial): count(f, t::Tuple) counts elements for which f is true.
# The Array/builtin count HOF did not accept a tuple, so count(iseven, (1,2,3,4))
# was an "Unknown function: count" error.

@testset "count(f, tuple) (Issue #5681)" begin
    @test count(iseven, (1, 2, 3, 4)) == 2
    @test count(x -> x > 2, (1, 2, 3, 4, 5)) == 3
    @test count(isodd, (1, 3, 5)) == 3
    @test count(iseven, ()) == 0
    @test count(x -> x == 0, (0, 0, 1, 0)) == 3

    # Array / range count are unchanged.
    @test count(iseven, [1, 2, 3, 4]) == 2
    @test count(iseven, 1:4) == 2
end

true
