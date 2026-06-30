# Generator iter-source parity (Issue #5088)
#
# Issue #5088 removed the per-access `(*g.iter).clone()` deep clone in the
# array_index Generator hot path, resolving the underlying iterator by
# reference instead. This fixture guards that generators over Range, Array,
# and Tuple iterators continue to collect/sum correctly, and that repeated
# consumption does not corrupt iterator state.

using Test

@testset "generator iter sources collect parity (Issue #5088)" begin
    # Range-backed generator
    @test collect(x + 1 for x in 1:4) == [2, 3, 4, 5]
    @test sum(x for x in 1:100) == 5050

    # Array-backed generator
    arr = [10, 20, 30]
    @test collect(x * 2 for x in arr) == [20, 40, 60]

    # Tuple-backed generator
    tup = (5, 6, 7)
    @test collect(x - 1 for x in tup) == [4, 5, 6]

    # Nested / repeated consumption must not corrupt iter state
    g = (x^2 for x in 1:5)
    @test collect(g) == [1, 4, 9, 16, 25]
    @test sum(y for y in (x^2 for x in 1:5)) == 55
end

true
