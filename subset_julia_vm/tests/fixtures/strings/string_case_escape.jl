# Test string functions: lowercasefirst, uppercasefirst, escape_string

using Test

@testset "lowercasefirst, uppercasefirst, escape_string" begin

    # === lowercasefirst(s) - convert first character to lowercase ===

    # Uppercase first character
    @assert lowercasefirst("Hello") == "hello"
    @assert lowercasefirst("WORLD") == "wORLD"
    @assert lowercasefirst("ABC") == "aBC"

    # Already lowercase first character
    @assert lowercasefirst("hello") == "hello"
    @assert lowercasefirst("world") == "world"

    # Single character
    @assert lowercasefirst("A") == "a"
    @assert lowercasefirst("a") == "a"

    # Empty string
    @assert lowercasefirst("") == ""

    # Non-letter first character
    @assert lowercasefirst("123abc") == "123abc"
    @assert lowercasefirst(" Hello") == " Hello"

    # === uppercasefirst(s) - convert first character to uppercase ===

    # Lowercase first character
    @assert uppercasefirst("hello") == "Hello"
    @assert uppercasefirst("world") == "World"
    @assert uppercasefirst("abc") == "Abc"

    # Already uppercase first character
    @assert uppercasefirst("Hello") == "Hello"
    @assert uppercasefirst("WORLD") == "WORLD"

    # Single character
    @assert uppercasefirst("a") == "A"
    @assert uppercasefirst("A") == "A"

    # Empty string
    @assert uppercasefirst("") == ""

    # Non-letter first character
    @assert uppercasefirst("123abc") == "123abc"
    @assert uppercasefirst(" hello") == " hello"

    # === escape_string(s) - escape special characters ===

    # Basic escaping
    @assert escape_string("hello") == "hello"
    @assert escape_string("world") == "world"

    # Backslash
    @assert escape_string("a\\b") == "a\\\\b"

    # Double quotes
    @assert escape_string("a\"b") == "a\\\"b"

    # Newline and tab
    @assert escape_string("a\nb") == "a\\nb"
    @assert escape_string("a\tb") == "a\\tb"

    # Carriage return
    @assert escape_string("a\rb") == "a\\rb"

    # Empty string
    @assert escape_string("") == ""

    # Multiple special characters
    @assert escape_string("a\n\tb") == "a\\n\\tb"

    # All tests passed
    @test (true)
end

true  # Test passed
