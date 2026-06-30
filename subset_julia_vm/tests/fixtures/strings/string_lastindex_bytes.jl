using Test

# Regression test for Issue #3662:
# `lastindex(s::String)` must return the last byte index (`ncodeunits(s)`),
# not the character count (`length(s)`). String indexing is byte-based, so
# `s[i:end]` truncates by extra UTF-8 continuation bytes when `lastindex`
# is wrong.

@testset "lastindex(::String) byte vs char (#3662)" begin
    s = "élan"
    @test length(s) == 4
    @test ncodeunits(s) == 5
    @test lastindex(s) == 5
    @test s[3:end] == "lan"
    @test last(s) == 'n'

    # ASCII regression (length == ncodeunits, no observable change)
    @test lastindex("abc") == 3
    @test "abc"[2:end] == "bc"
    @test last("abc") == 'c'

    # Empty string
    @test lastindex("") == 0

    # Multi-byte chars throughout
    t = "über"
    @test lastindex(t) == 5      # ü is 2 bytes
    @test t[3:end] == "ber"

    # Mixed ASCII + non-ASCII
    u = "a漢b"
    @test ncodeunits(u) == 5     # a=1 + 漢=3 + b=1
    @test lastindex(u) == 5
    @test u[5:end] == "b"
end

true
