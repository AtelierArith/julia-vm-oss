# Test unescape_string function (Issue #2086)

using Test

@testset "unescape_string(s) - unescape string escape sequences" begin

    # === Basic escape sequences ===

    # Newline
    @test unescape_string("hello\\nworld") == "hello\nworld"

    # Tab
    @test unescape_string("hello\\tworld") == "hello\tworld"

    # Carriage return
    @test unescape_string("hello\\rworld") == "hello\rworld"

    # Backslash
    @test unescape_string("hello\\\\world") == "hello\\world"

    # Quote
    @test unescape_string("hello\\\"world") == "hello\"world"

    # === No escape sequences ===
    @test unescape_string("hello world") == "hello world"
    @test unescape_string("") == ""
    @test unescape_string("abc") == "abc"

    # === Multiple escapes ===
    @test unescape_string("a\\nb\\tc") == "a\nb\tc"

    # === Hex escape ===
    @test unescape_string("\\x41") == "A"  # 0x41 = 'A'
    @test unescape_string("\\x48\\x69") == "Hi"
end

true  # Test passed
