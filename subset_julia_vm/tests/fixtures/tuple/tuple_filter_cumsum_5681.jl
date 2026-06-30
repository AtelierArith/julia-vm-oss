using Test

# Issue #5681: filter / cumsum / cumprod over a Tuple must return a Tuple. The
# subset has no tuple splat and no Tuple(::Vector), so the result is collected into
# an Any[] (type-preserving since #5717) and rebuilt by output arity.

@testset "filter/cumsum/cumprod on tuples (Issue #5681)" begin
    @test filter(iseven, (1, 2, 3, 4)) == (2, 4)
    @test filter(isodd, (1, 2, 3, 4, 5)) == (1, 3, 5)
    @test filter(iseven, (1, 3, 5)) == ()
    @test filter(iseven, ()) == ()
    @test filter(x -> x > 2, (1, 2, 3, 4)) == (3, 4)
    @test filter(iseven, (1, 2, 3, 4)) isa Tuple

    @test cumsum((1, 2, 3)) == (1, 3, 6)
    @test cumsum((1, 2, 3, 4)) == (1, 3, 6, 10)
    @test cumsum((1.0, 2.0, 3.0)) == (1.0, 3.0, 6.0)
    @test cumsum(()) == ()
    @test cumsum((1, 2, 3)) isa Tuple

    @test cumprod((1, 2, 3, 4)) == (1, 2, 6, 24)
    @test cumprod((2, 3)) == (2, 6)
    @test cumprod(()) == ()

    # Element types are preserved (not widened to Float64).
    @test typeof(filter(iseven, (1, 2, 3, 4))[1]) == Int64
    @test typeof(cumsum((1, 2, 3))[1]) == Int64

    # Larger tuple (exercises the arity-rebuild path).
    @test filter(iseven, (1, 2, 3, 4, 5, 6, 7, 8, 9, 10)) == (2, 4, 6, 8, 10)
end

true
