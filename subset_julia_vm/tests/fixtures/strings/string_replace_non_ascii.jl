using Test

# Regression test for Issue #3607:
# `replace(s, old => new)` previously corrupted non-ASCII output by mixing
# `length` (char count) with `codeunit` (byte index) and re-emitting each
# UTF-8 byte as a separate Char. Now decodes the full multi-byte char on
# no-match and uses byte-level (`ncodeunits`) bounds throughout.

@testset "replace non-ASCII pattern (#3607)" begin
    # MWE: distinct multi-byte chars sharing leading byte
    @test replace("éê", "é" => "x") == "xê"

    # Multi-byte pattern in longer string
    @test replace("café", "é" => "e") == "cafe"
    @test replace("café", "é" => "É") == "cafÉ"
    @test replace("café au lait", "café" => "tea") == "tea au lait"

    # CJK pattern
    @test replace("漢字漢", "漢" => "X") == "X字X"
    @test replace("漢字", "字" => "ZI") == "漢ZI"

    # Non-ASCII with empty replacement
    @test replace("aéa", "é" => "") == "aa"

    # Surrounding chars preserved correctly
    @test replace("aébc", "é" => "X") == "aXbc"
    @test replace("aécd", "c" => "Y") == "aéYd"

    # ASCII regression
    @test replace("Hello", "l" => "L") == "HeLLo"
    @test replace("hello", "ll" => "LL") == "heLLo"
    @test replace("aaaa", "a" => "b"; count=2) == "bbaa"

    # No-match cases
    @test replace("hello", "x" => "y") == "hello"
    @test replace("éê", "ê" => "X") == "éX"
    @test replace("éê", "x" => "Y") == "éê"

    # Edge cases
    @test replace("", "a" => "b") == ""
end

true
