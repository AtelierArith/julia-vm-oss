# Test custom string macros (Issue #468)
# Tests that prefix"text" calls the prefix_str function

using Test

# Define a custom string macro function
function foo_str(s::String)
    "FOO:" * s
end

# Define another custom string macro
function upper_str(s::String)
    uppercase(s)
end

@testset "Custom string macros" begin
    # Test custom foo"..." literal
    result = foo"hello"
    @test result == "FOO:hello"

    # Test custom upper"..." literal
    result2 = upper"hello"
    @test result2 == "HELLO"

    # Verify it works with different content
    @test foo"world" == "FOO:world"
    @test upper"world" == "WORLD"

    # Empty strings
    @test foo"" == "FOO:"
    @test upper"" == ""
end

true
