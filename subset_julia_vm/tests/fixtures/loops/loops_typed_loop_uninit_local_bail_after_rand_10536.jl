# Issue #10536: a typed loop that draws `rand()` and then reads a
# path-dependent loop local left uninitialized on every reachable path (the
# condition guarding its only assignment can never be true within the loop)
# must not double-draw `rand()` when the resulting UndefVarError is caught by
# an enclosing try/catch.
#
# Before the fix, the typed-loop fast path drew `rand()` once, bailed to the
# generic interpreter on the uninitialized read, and the generic re-run drew
# `rand()` AGAIN before raising the same error — shifting every subsequent
# `rand()` draw by one relative to upstream Julia (Issue #10504's guard
# intentionally excludes uninit-loop-local loads from the bail-capable set so
# `RandF64` loops keep the native path; this is the residual it leaves
# behind).

using Test
using Random

function f_10536(n::Int64)::Float64
    s = 0.0
    i = 1
    while i <= n
        x = rand()
        if i > n
            t = 1.0
        end
        s = s + x + t
        i = i + 1
    end
    s
end

Random.seed!(0)
caught = false
caught_name = :none
try
    f_10536(5)
catch e
    global caught = true
    global caught_name = e.var
end
next_draw = rand()

Random.seed!(0)
first_draw = rand()
second_draw = rand()

@testset "typed loop uninit-local bail does not double-draw rand() (Issue #10536)" begin
    @test caught
    @test caught_name === :t
    @test first_draw != second_draw
    # Ground truth: the loop's only iteration draws exactly ONE rand() (`x`)
    # before bailing on the uninitialized `t` read, so the very next rand()
    # call after the catch must return the stream's SECOND draw, not its
    # third (the double-draw symptom of the bug).
    @test next_draw == second_draw
end

true
