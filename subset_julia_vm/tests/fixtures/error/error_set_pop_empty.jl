using Test

# Tests that pop! on an empty Set raises a catchable error (Issue #3068)
# In Julia: pop!(Set{Int}()) throws ArgumentError("Set must be non-empty")

function pop_empty_set_caught()
    caught = false
    try
        s = Set{Int}()
        pop!(s)  # ArgumentError: Set must be non-empty
    catch e
        caught = true
    end
    return caught
end

function pop_nonempty_set_ok()
    # pop! on non-empty Set should NOT raise and should remove an element
    caught = false
    s = Set([1, 2, 3])
    try
        val = pop!(s)
        caught = false
    catch e
        caught = true
    end
    return !caught && length(s) == 2
end

@testset "pop! on empty Set raises catchable error (Issue #3068)" begin
    @test pop_empty_set_caught()
    @test pop_nonempty_set_ok()
end

true
