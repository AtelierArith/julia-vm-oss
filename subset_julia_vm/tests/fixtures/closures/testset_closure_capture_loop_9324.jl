using Test

# Issue #9324: closures capturing a `for`-loop variable inside a `@testset` body
# get a FRESH per-iteration binding — exactly like a source-level `let`, a
# function body, or plain top level — instead of sharing one cell that ends at
# the post-loop value. Before the fix the quote-expanded synthetic-marker `let`
# that a `@testset` body becomes did not register the loop variable as a
# capturable local, so every closure read `i` as a post-loop module global (all
# three printed 30). Verified at parity with julia 1.12.
#
# The closures are collected into a module-global vector and re-checked with
# top-level `@assert`s after the testset: a failing `@test` inside a `@testset`
# does not throw in sjulia (the `expected = true` harness checks only the final
# value), whereas `@assert` throws and is caught by the harness.

fs = []
@testset "per-iteration closure capture in a @testset for-loop (Issue #9324)" begin
    for i in 1:3
        push!(fs, () -> i * 10)
    end
    @test fs[1]() == 10
    @test fs[2]() == 20
    @test fs[3]() == 30
end
@assert fs[1]() == 10 "closures shared one loop-variable cell (Issue #9324)"
@assert fs[2]() == 20 "closures shared one loop-variable cell (Issue #9324)"
@assert fs[3]() == 30 "closures shared one loop-variable cell (Issue #9324)"

# The same fresh-binding semantics for a comma-tuple loop variable.
gs = []
@testset "per-iteration capture of a comma-tuple loop variable" begin
    for (a, b) in [(1, 2), (3, 4)]
        push!(gs, () -> a * 10 + b)
    end
    @test gs[1]() == 12
    @test gs[2]() == 34
end
@assert gs[1]() == 12 "tuple-loop closures shared one cell (Issue #9324)"
@assert gs[2]() == 34 "tuple-loop closures shared one cell (Issue #9324)"

true
