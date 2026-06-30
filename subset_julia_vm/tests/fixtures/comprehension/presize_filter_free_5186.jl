# Filter-free comprehension pre-sizing (Issue #5186)
#
# A filter-free comprehension `[f(x) for x in iter]` has a final length equal
# to `length(iter)`, so the compiler now reserves the backing storage up front
# (via the `ReserveArray` instruction) instead of growing it with O(log n)
# reallocations. `ReserveArray` is a pure capacity hint, so the observable
# results and element types must be IDENTICAL to before. This fixture guards
# that correctness across element types and iterator kinds.

using Test

@testset "Filter-free comprehension pre-sizing preserves results (Issue #5186)" begin
    # Int64 body over a range
    a = [x for x in 1:7]
    @test a == [1, 2, 3, 4, 5, 6, 7]
    @test typeof(a) == Vector{Int64}
    @test length(a) == 7

    # Int64 expression over a range
    b = [2 * x + 1 for x in 1:6]
    @test b == [3, 5, 7, 9, 11, 13]
    @test typeof(b) == Vector{Int64}

    # Float64 body
    c = [x / 2 for x in 1:5]
    @test c == [0.5, 1.0, 1.5, 2.0, 2.5]
    @test typeof(c) == Vector{Float64}

    # String body
    d = [string(x) for x in 1:4]
    @test d == ["1", "2", "3", "4"]
    @test typeof(d) == Vector{String}

    # Bool body
    e = [iseven(x) for x in 1:5]
    @test e == [false, true, false, true, false]
    @test typeof(e) == Vector{Bool}

    # Char body. NOTE: only the produced values are asserted here, not the
    # inferred element type — `[Char(x+96) for x in 1:5]` is currently inferred
    # as `Vector{Any}` by sjulia (the `Char(...)` call return type is not
    # statically resolved), a pre-existing inference limitation independent of
    # the pre-sizing change. Pre-sizing must leave the produced values intact.
    g = [Char(x + 96) for x in 1:5]
    @test g == ['a', 'b', 'c', 'd', 'e']
    @test length(g) == 5

    # Comprehension over an existing array iterator
    src = [10, 20, 30, 40]
    h = [v + 1 for v in src]
    @test h == [11, 21, 31, 41]
    @test typeof(h) == Vector{Int64}

    # Larger range to exercise growth path (would realloc multiple times)
    big = [x * x for x in 1:100]
    @test length(big) == 100
    @test big[1] == 1
    @test big[100] == 10000
    @test sum(big) == 338350
    @test typeof(big) == Vector{Int64}

    # Empty range: reserve count is 0, must stay empty
    empty_comp = [x for x in 1:0]
    @test length(empty_comp) == 0
    @test typeof(empty_comp) == Vector{Int64}

    # Single element
    one = [x + 5 for x in 3:3]
    @test one == [8]
    @test typeof(one) == Vector{Int64}
end

true
