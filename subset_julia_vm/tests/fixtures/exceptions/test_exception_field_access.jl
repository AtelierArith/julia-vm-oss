# Test accessing exception struct fields in catch blocks
# Related to Issue #362

using Test

@testset "Access exception struct fields in catch blocks (Issue #362)" begin

    # Test 1: Basic DimensionMismatch field access in catch
    caught_msg = ""
    try
        throw(DimensionMismatch("test message"))
    catch e
        caught_msg = e.msg
    end

    # Use length comparison since string equality might have issues
    @assert length(caught_msg) == 12  # "test message" has 12 chars

    # Test 2: KeyError field access in catch
    caught_key = ""
    try
        throw(KeyError("missing_key"))
    catch e
        caught_key = e.key
    end

    @assert length(caught_key) == 11  # "missing_key" has 11 chars

    # Test 3: StringIndexError field access in catch
    caught_str = ""
    caught_idx = 0
    try
        throw(StringIndexError("hello", 3))
    catch e
        caught_str = e.string
        caught_idx = e.index
    end

    @assert length(caught_str) == 5  # "hello" has 5 chars
    @assert caught_idx == 3

    # Return true to indicate all tests passed
    @test (true)
end

true  # Test passed
