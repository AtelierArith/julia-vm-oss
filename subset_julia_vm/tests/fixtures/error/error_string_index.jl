using Test

# Tests for StringIndexError when accessing invalid string byte indices (Issue #3046)
# Julia strings are UTF-8 encoded; only valid character boundaries are allowed.

function string_valid_index_no_error()
    s = "hello"
    caught = false
    try
        c = s[1]  # Valid index
    catch e
        caught = true
    end
    return caught
end

function string_length_index_valid()
    s = "abc"
    caught = false
    try
        c = s[3]  # Last valid index
    catch e
        caught = true
    end
    return caught
end

function string_out_of_bounds_caught()
    s = "hello"
    caught = false
    try
        c = s[10]  # Out of bounds
    catch e
        caught = true
    end
    return caught
end

function string_zero_index_caught()
    s = "hello"
    caught = false
    try
        c = s[0]  # Zero index is invalid in Julia (1-based)
    catch e
        caught = true
    end
    return caught
end

@testset "string index errors are catchable (Issue #3046)" begin
    @test !string_valid_index_no_error()
    @test !string_length_index_valid()
    @test string_out_of_bounds_caught()
    @test string_zero_index_caught()
end

true
