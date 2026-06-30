using Test

# Issue #6865: a cartesian `for x in xs, y in ys ... end` (multiple
# comma-separated iterators in a single `for`) used to fail in lowering with
# `UnsupportedForBinding`. It now desugars to nested loops, exactly as upstream
# Julia's `expand-for` expands multiple iterators: the first binding is the
# outermost loop and the last is the innermost, and an inner iterator may refer
# to variables bound by an outer one.

@testset "cartesian for (Issue #6865)" begin
    # Two iterators over ranges.
    s = 0
    for x in 1:3, y in 1:3
        s += x + y
    end
    @test s == 36

    # Three iterators.
    t = 0
    for i in 1:2, j in 1:2, k in 1:2
        t += i * 100 + j * 10 + k
    end
    @test t == 1332

    # Inner iterator depends on the outer loop variable.
    u = 0
    for i in 1:3, j in 1:i
        u += j
    end
    @test u == 10

    # Mixed iterables: an array of strings on the outside, a range inside.
    acc = String[]
    for c in ["a", "b"], k in 1:2
        push!(acc, string(c, k))
    end
    @test acc == ["a1", "a2", "b1", "b2"]

    # Tuple destructuring on the outer binding, range on the inner one.
    total = 0
    for (a, b) in [(1, 2), (3, 4)], j in 1:2
        total += a + b + j
    end
    @test total == 26

    # A float-stepped inner range still iterates correctly.
    r = 0.0
    for i in 1:2, x in 1.0:0.5:2.0
        r += x
    end
    @test r == 9.0
end

true
