# Test strip, lstrip, rstrip functions

using Test

@testset "strip, lstrip, rstrip - remove leading/trailing whitespace" begin

    # lstrip - remove leading whitespace
    @assert lstrip("  hello") == "hello"
    @assert lstrip("\t\nhello") == "hello"
    @assert lstrip("hello") == "hello"
    @assert lstrip("   ") == ""
    @assert lstrip("") == ""

    # rstrip - remove trailing whitespace
    @assert rstrip("hello  ") == "hello"
    @assert rstrip("hello\t\n") == "hello"
    @assert rstrip("hello") == "hello"
    @assert rstrip("   ") == ""
    @assert rstrip("") == ""

    # strip - remove both leading and trailing whitespace
    @assert strip("  hello  ") == "hello"
    @assert strip("\t\nhello\t\n") == "hello"
    @assert strip("hello") == "hello"
    @assert strip("   ") == ""
    @assert strip("") == ""

    # Mixed whitespace
    @assert strip("  hello world  ") == "hello world"
    @assert lstrip("  hello world  ") == "hello world  "
    @assert rstrip("  hello world  ") == "  hello world"

    @test (true)
end

true  # Test passed
