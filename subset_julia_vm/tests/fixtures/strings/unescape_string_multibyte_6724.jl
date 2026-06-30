# Issue #6724: unescape_string migrated from a Rust builtin to pure Julia
# (base/strings/util.jl). The previous Rust handler mixed byte/char indexing
# and corrupted multibyte input — e.g. unescape_string("café\\n") produced
# "cafÃ©\n" and dropped the trailing character. The pure-Julia version
# iterates over characters, so multibyte text is preserved while ASCII escape
# sequences are still decoded. Values verified against upstream julia 1.12.

using Test

@testset "unescape_string preserves multibyte text (Issue #6724)" begin
    @test unescape_string("café\\n end") == "café\n end"
    @test unescape_string("caf\\u00e9 \\t x") == "café \t x"
    @test unescape_string("αβγ\\tδ") == "αβγ\tδ"
    @test unescape_string("emoji 😀 done") == "emoji 😀 done"
    @test unescape_string("π=\\x33") == "π=3"
    # multibyte char immediately before and after an escape
    @test unescape_string("好\\n世") == "好\n世"
end

@testset "unescape_string ASCII escapes regression (Issue #6724)" begin
    @test unescape_string("hello\\nworld") == "hello\nworld"
    @test unescape_string("a\\tb\\rc") == "a\tb\rc"
    @test unescape_string("hello\\\\world") == "hello\\world"
    @test unescape_string("hello\\\"world") == "hello\"world"
    @test unescape_string("\\x41\\x42") == "AB"
    @test unescape_string("\\u00e9") == "é"
    @test unescape_string("\\U0001F600") == "😀"
    @test unescape_string("") == ""
    @test unescape_string("abc") == "abc"
end

true
