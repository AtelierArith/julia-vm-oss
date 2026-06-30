using Test

# Regression tests for Issue #3605:
# `findfirst(::String, ::String)` (and `findnext`/`findlast`/`findprev`)
# previously returned the byte-length range `i:i+ncodeunits(pattern)-1`.
# For non-ASCII patterns this overshoots — Julia's returned UnitRange ends
# at the byte index of the *start* of the last matched character, so e.g.
# `findfirst("é", "éa")` should be `1:1`, not `1:2`.

# UnitRange `==` is not yet implemented in subset Julia VM, so compare
# endpoints with `first`/`last` (matching `string_findnext_findprev.jl`).

function range_eq(r, lo, hi)
    return first(r) == lo && last(r) == hi
end

@testset "findfirst non-ASCII range (#3605)" begin
    # MWE: one-character non-ASCII pattern (2 bytes)
    @test range_eq(findfirst("é", "éa"), 1, 1)
    @test range_eq(findfirst("é", "aé"), 2, 2)
    @test range_eq(findfirst("é", "ééé"), 1, 1)

    # Multi-character non-ASCII pattern: end is byte index of last char start.
    # "éa" has chars at bytes 1 and 3 within the pattern, so the range spans
    # `i:(i+2)`.
    @test range_eq(findfirst("éa", "béa"), 2, 4)
    @test range_eq(findfirst("éé", "aéé"), 2, 4)

    # 3-byte chars (CJK)
    @test range_eq(findfirst("漢", "中漢字"), 4, 4)
    @test range_eq(findfirst("漢字", "中漢字"), 4, 7)
    @test range_eq(findfirst("中", "中漢字"), 1, 1)

    # ASCII regression: byte length and char-boundary length coincide.
    @test range_eq(findfirst("a", "abc"), 1, 1)
    @test range_eq(findfirst("ab", "xab"), 2, 3)
    @test range_eq(findfirst("bc", "abcabc"), 2, 3)
    @test findfirst("xyz", "abcabc") === nothing

    # Empty pattern: `i:i-1` (1:0)
    @test range_eq(findfirst("", "abc"), 1, 0)
end

@testset "findlast non-ASCII range (#3605)" begin
    @test range_eq(findlast("é", "aée"), 2, 2)
    @test range_eq(findlast("é", "ééé"), 5, 5)
    @test range_eq(findlast("éé", "aéé"), 2, 4)
    @test range_eq(findlast("漢", "中漢字"), 4, 4)
    @test range_eq(findlast("a", "abcabc"), 4, 4)
    @test range_eq(findlast("ab", "abcabc"), 4, 5)
    @test findlast("xyz", "abcabc") === nothing
end

@testset "findnext/findprev non-ASCII range (#3605)" begin
    # Resume search at the next valid string index (byte index of a char start)
    # "éaé": chars at bytes 1, 3, 4 — start search at 3 to skip the first 'é'.
    @test range_eq(findnext("é", "éaé", 3), 4, 4)
    # "aéaé": chars at bytes 1, 2, 4, 5 — search past the last 'é' returns nothing.
    @test findnext("é", "aéaé", 7) === nothing

    # "éaéa": chars at bytes 1, 3, 4, 6 — search backwards from byte 4 ('é').
    @test range_eq(findprev("é", "éaéa", 4), 4, 4)
    @test range_eq(findprev("é", "éaéa", 3), 1, 1)
end

true
