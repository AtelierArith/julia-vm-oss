# Regression test for Issue #7831:
# Statement-position `@sync for ... @async ... end` must recurse into the loop
# body, wrapping each spawned `@async` into the shared exceptions accumulator and
# awaiting it. Previously this body shape fell through to plain for-loop lowering
# with NO task collection/await, so every `@async` result was silently dropped
# (e.g. `@sync for ...; @async push!(results, ...); end` returned an empty array)
# instead of running correctly or raising a loud error. See CLAUDE.md principle 4.

using Test

@testset "@sync for over range spawning @async runs every body (Issue #7831)" begin
    results = Int[]
    @sync for i in 1:3
        @async push!(results, i^2)
    end
    @test sort(results) == [1, 4, 9]
end

@testset "@sync for over an iterable spawning @async" begin
    results = Int[]
    @sync for x in [10, 20, 30]
        @async push!(results, x + 1)
    end
    @test sort(results) == [11, 21, 31]
end

@testset "@sync for runs non-async body statements alongside @async" begin
    acc = Int[]
    @sync for i in 1:3
        y = i * 10
        @async push!(acc, y)
    end
    @test sort(acc) == [10, 20, 30]
end

@testset "@sync for with assigned `t = @async` awaits each task" begin
    out = Int[]
    @sync for i in 1:2
        t = @async push!(out, i)
    end
    @test sort(out) == [1, 2]
end

# A failure inside an `@async` spawned in the loop must surface loudly as a
# CompositeException, not be silently dropped.
@testset "@sync for aggregates @async failures into CompositeException" begin
    ex = nothing
    try
        @sync for i in 1:3
            @async (i == 2 && error("boom"); nothing)
        end
    catch e
        ex = e
    end
    @test isa(ex, CompositeException)
end

true
