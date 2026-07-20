# eachmatch(re, s; overlap=true) restarts the search one character past each
# match START (nextind(s, m.offset)) instead of past the match end, yielding
# overlapping matches — mirroring upstream Base.RegexMatchIterator (Issue #10199).

using Test

@testset "eachmatch overlap keyword" begin
    # The canonical MWE: three overlapping 2-char windows of "aaaa".
    @test [m.offset for m in eachmatch(r"aa", "aaaa"; overlap=true)] == [1, 2, 3]

    # Default (no kwarg) and overlap=false stay non-overlapping.
    @test [m.offset for m in eachmatch(r"aa", "aaaa")] == [1, 3]
    @test [m.offset for m in eachmatch(r"aa", "aaaa"; overlap=false)] == [1, 3]

    # Overlapping matches with a capture group (upstream `count` docstring case).
    @test [m.offset for m in eachmatch(r"a(.)a", "cabacabac"; overlap=true)] == [2, 4, 6]
    @test [m.match for m in eachmatch(r"a.a", "a1a2a3a"; overlap=true)] ==
          ["a1a", "a2a", "a3a"]

    # Captures and their 1-based offsets survive the overlapping restart.
    @test [m.captures[1] for m in eachmatch(r"a(.)a", "cabacabac"; overlap=true)] ==
          ["b", "c", "b"]
    @test [m.offsets[1] for m in eachmatch(r"a(.)a", "cabacabac"; overlap=true)] ==
          [3, 5, 7]

    # Multibyte haystack: offsets stay byte-based and 1-based.
    @test [m.offset for m in eachmatch(r"αα", "αααα"; overlap=true)] == [1, 3, 5]

    # Empty-capable pattern advances one character per position (no infinite
    # loop) and matches upstream exactly.
    @test [(m.offset, m.match) for m in eachmatch(r"a*", "baab"; overlap=true)] ==
          [(1, ""), (2, "aa"), (3, "a"), (4, ""), (5, "")]

    # count/findall (non-overlap wrappers) are unaffected.
    @test count(r"aa", "aaaa") == 2
end

true
