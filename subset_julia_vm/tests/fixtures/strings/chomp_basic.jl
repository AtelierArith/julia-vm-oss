# Test chomp function - remove trailing newline

using Test

@testset "chomp - remove ONE trailing newline (
 or 
)" begin

    # Remove \n
    @assert chomp("hello\n") == "hello"
    @assert chomp("hello world\n") == "hello world"

    # Remove \r\n (Windows-style)
    @assert chomp("hello\r\n") == "hello"

    # No newline - return as-is
    @assert chomp("hello") == "hello"
    @assert chomp("hello ") == "hello "

    # Empty string
    @assert chomp("") == ""

    # Only newline
    @assert chomp("\n") == ""
    @assert chomp("\r\n") == ""

    # Multiple newlines - only remove one
    @assert chomp("hello\n\n") == "hello\n"

    @test (true)
end

true  # Test passed
