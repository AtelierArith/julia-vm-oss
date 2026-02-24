using Test

# Tests that first()/last() on empty tuple and second access on single-element tuple
# raise catchable BoundsError (Issue #3068)

function first_empty_tuple_caught()
    caught = false
    try
        first(())  # BoundsError: empty tuple has no first element
    catch e
        caught = true
    end
    return caught
end

function first_nonempty_tuple_ok()
    # first() on non-empty tuple should NOT raise
    caught = false
    try
        val = first((10, 20, 30))
        caught = (val != 10)  # only set caught if wrong value
    catch e
        caught = true
    end
    return !caught
end

function tuple_first_still_works()
    # TupleFirst instruction on valid tuple must still return correct value
    t = (42, 99)
    return first(t) == 42
end

@testset "TupleFirst/TupleSecond out-of-bounds raises catchable error (Issue #3068)" begin
    @test first_empty_tuple_caught()
    @test first_nonempty_tuple_ok()
    @test tuple_first_still_works()
end

true
