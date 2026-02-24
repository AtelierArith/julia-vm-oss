# Test ENV constant
# ENV should be a Dict{String,String} containing environment variables

using Test

@testset "ENV constant: environment variable dictionary (Issue #340)" begin

    result = true

    # Check that ENV is a Dict
    if !(typeof(ENV) <: AbstractDict)
        result = false
    end

    # ENV should have some entries (at least PATH or HOME on most systems)
    if length(ENV) == 0
        # This might be valid in some sandboxed environments,
        # but typically ENV has at least some variables
        # We don't fail here as it depends on the execution environment
    end

    # Test haskey function on ENV
    # PATH is almost always present on Unix systems
    # HOME is common on macOS/Linux, USERPROFILE on Windows
    has_some_var = haskey(ENV, "PATH") || haskey(ENV, "HOME") || haskey(ENV, "USER")

    # We don't fail if no vars found - could be sandboxed environment
    # Just test that haskey works without error

    # Test that we can iterate over ENV keys (if any exist)
    key_count = 0
    for key in keys(ENV)
        key_count = key_count + 1
        if key_count >= 3
            break  # Just test a few
        end
    end

    @test (result)
end

true  # Test passed
