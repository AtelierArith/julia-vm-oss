using Test

# Regression test for Issue #3606:
# `chopprefix` (and the sister `chopsuffix`) must use byte counts
# (`ncodeunits`) when slicing, not character counts (`length`). Otherwise
# multi-byte UTF-8 prefixes/suffixes split inside a character and trigger
# StringIndexError.

@testset "chopprefix non-ASCII (#3606)" begin
    # MWE
    @test chopprefix("éa", "é") == "a"

    # Multi-byte prefix, longer body
    @test chopprefix("café", "ca") == "fé"
    @test chopprefix("漢字abc", "漢字") == "abc"

    # Repeated non-ASCII char
    @test chopprefix("éé", "é") == "é"

    # No match: returned unchanged
    @test chopprefix("hello", "x") == "hello"
    @test chopprefix("éhello", "x") == "éhello"

    # Empty prefix
    @test chopprefix("hello", "") == "hello"

    # ASCII regression
    @test chopprefix("hello", "he") == "llo"
    @test chopprefix("hello", "hello") == ""
end

@testset "chopsuffix non-ASCII (sibling of #3606)" begin
    @test chopsuffix("aé", "é") == "a"
    @test chopsuffix("café", "fé") == "ca"
    @test chopsuffix("abc漢字", "漢字") == "abc"
    @test chopsuffix("éé", "é") == "é"

    # No match
    @test chopsuffix("hello", "x") == "hello"

    # Empty suffix
    @test chopsuffix("hello", "") == "hello"

    # ASCII regression
    @test chopsuffix("hello", "lo") == "hel"
    @test chopsuffix("hello", "hello") == ""
end

true
