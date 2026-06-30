# Test: Loop body return must NOT short-circuit function inference (Issue #3516).
# A `return` inside a loop body only fires if the loop iterates; the post-loop
# fallthrough must remain reachable. The inferred return type should be the
# union of body-return and post-loop-fallthrough types.

using Test

# Function whose only `return` is inside a for-range loop. With an empty range
# the loop body never executes, so the post-loop `0` must be reached.
function f_for_range(stop::Int)
    for i in 1:stop
        return "hit"
    end
    0
end

# Same idea with for-each over an array argument.
function f_foreach(xs::Vector{Int})
    for x in xs
        return "hit"
    end
    0
end

# While-loop variant: condition false from the start.
function f_while(go::Bool)
    while go
        return "hit"
    end
    0
end

@testset "Loop body return + post-loop fallthrough (Issue #3516)" begin
    # Empty range: loop body skipped, falls through to 0.
    @test f_for_range(0) === 0
    # Non-empty range: returns from loop body.
    @test f_for_range(1) == "hit"

    # Empty array: foreach body skipped.
    @test f_foreach(Int[]) === 0
    # Non-empty array: returns from loop body.
    @test f_foreach([1, 2]) == "hit"

    # while condition false from start: never enters loop.
    @test f_while(false) === 0
    # while condition true: enters body and returns.
    @test f_while(true) == "hit"
end

true  # Test passed
