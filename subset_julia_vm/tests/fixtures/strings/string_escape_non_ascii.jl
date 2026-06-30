using Test

# Regression test for Issue #3599:
# `escape_string` previously iterated UTF-8 bytes and converted each to a
# Char, mangling multi-byte chars (e.g. "é" → "Ã"). Now iterates characters.

@testset "escape_string non-ASCII (#3599)" begin
    # Single non-ASCII character (MWE)
    @test escape_string("é") == "é"
    @test escape_string("Ω") == "Ω"
    @test escape_string("漢") == "漢"

    # Multi-character non-ASCII
    @test escape_string("café") == "café"
    @test escape_string("漢字") == "漢字"
    @test escape_string("Hello, 世界") == "Hello, 世界"

    # Mixed ASCII + non-ASCII with escape characters
    @test escape_string("é\nü") == "é\\nü"
    @test escape_string("漢\t字") == "漢\\t字"

    # ASCII regression — escaping must still work
    @test escape_string("hello") == "hello"
    @test escape_string("a\nb") == "a\\nb"
    @test escape_string("\\") == "\\\\"
    @test escape_string("\"") == "\\\""
    @test escape_string("\t") == "\\t"
    @test escape_string("\r") == "\\r"
    @test escape_string("") == ""
end

true
