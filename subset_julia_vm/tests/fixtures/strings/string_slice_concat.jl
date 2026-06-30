using Test

# Regression tests for Issue #3671:
# Inside a function body, the binary `*` operator falls through to
# `dynamic_mul`, which previously had no case for `Value::Str * Value::Str`
# (or Char). Concatenating a slice result with another String produced
# `Cannot multiply "String" and "String"` even though both operands were
# concretely `String`. Same path also broke `String * Char` and `Char * Char`.

function _slice_concat(s)
    result = ""
    chunk = s[2:4]
    return result * chunk
end

function _string_times_char(s)
    return "" * s[1]
end

function _char_times_string(s)
    return s[1] * ""
end

function _char_times_char(s)
    return s[1] * s[2]
end

function _slice_times_slice(s)
    return s[1:2] * s[3:4]
end

function _slice_times_literal(s)
    return s[2:4] * "!"
end

@testset "String slice * String concat (#3671)" begin
    @test _slice_concat("hello") == "ell"
    @test _slice_times_slice("hello") == "hell"
    @test _slice_times_literal("hello") == "ell!"

    # Non-ASCII slice: char positions in "aéabcd" are 1, 2, 4, 5, 6, 7.
    # `s[2:4]` covers 'é' (bytes 2..3) and 'a' (byte 4) → "éa".
    @test _slice_concat("aéabcd") == "éa"
end

@testset "String * Char and Char * String (#3671)" begin
    @test _string_times_char("hello") == "h"
    @test _char_times_string("hello") == "h"
    @test _char_times_char("hello") == "he"

    # Non-ASCII chars
    @test _string_times_char("éhi") == "é"
    # "aéh": char positions 1, 2, 4 — s[1] = 'a', s[2] = 'é'.
    @test _char_times_char("aéh") == "aé"
end

@testset "Plain string concat regression" begin
    # These already worked; ensure the new dynamic_mul cases don't regress them.
    @test "a" * "b" == "ab"
    @test "ab" * "cd" == "abcd"
    @test "" * "hello" == "hello"
    @test "hello" * "" == "hello"
end

true
