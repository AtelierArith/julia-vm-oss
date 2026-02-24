using Test

# Tests that out-of-bounds indexing on a Pair raises a catchable BoundsError (Issue #3062)
# Julia Pairs have length 2; indexing beyond that raises BoundsError

function pair_bounds_caught_struct()
    caught = false
    try
        pair = "key" => 42
        x = pair[3]  # BoundsError: index 3 out of range for Pair (length 2)
    catch e
        caught = true
    end
    return caught
end

function pair_bounds_caught_index_zero()
    caught = false
    try
        pair = :a => 100
        x = pair[0]  # BoundsError: index 0 out of range for Pair (length 2)
    catch e
        caught = true
    end
    return caught
end

function pair_valid_access_no_catch()
    # Valid accesses to Pair elements should NOT raise
    caught = false
    try
        pair = "hello" => 99
        first_elem = pair[1]   # "hello"
        second_elem = pair[2]  # 99
    catch e
        caught = true
    end
    return caught
end

@testset "Pair index out of bounds raises catchable error (Issue #3062)" begin
    @test pair_bounds_caught_struct()
    @test pair_bounds_caught_index_zero()
    @test !pair_valid_access_no_catch()
end

true
