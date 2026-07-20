# Regex(pattern[, flags]) constructor called as a function, and the 3-arg
# match(re, s, start) offset search (Issue #10178). Both entry points errored
# with "Unknown function: ..." even though the RegexNew / RegexMatch builtins
# already existed — only the r"..." literal path was wired up.

using Test

@testset "Regex constructor and 3-arg match (Issue #10178)" begin
    # --- Regex(pattern) constructor (1-arg) ---
    r1 = Regex("ab+c")
    @test r1 isa Regex
    @test occursin(r1, "xabbcy")
    @test !occursin(r1, "xac")
    @test occursin(Regex("ab+c"), "xabbcy")   # directly nested

    # --- Regex(pattern, flags) constructor (2-arg) ---
    r2 = Regex("abc", "i")
    @test occursin(r2, "ABC")
    @test occursin(r2, "abc")
    @test !occursin(Regex("abc"), "ABC")       # no i flag -> no match

    # --- Dynamic pattern building ---
    digits = Regex("[0-9]+")
    m = match(digits, "abc123def")
    @test m.match == "123"
    @test m.offset == 4

    # --- 3-arg match(re, s, start): search from a 1-based byte offset ---
    m1 = match(r"bc", "abcbc", 4)
    @test m1.offset == 4
    @test m1.match == "bc"

    # Same regex, default start finds the first occurrence.
    @test match(r"bc", "abcbc").offset == 2
    @test match(r"bc", "abcbc", 1).offset == 2

    # Starting past the first match skips it.
    m2 = match(r"o", "hello world", 6)
    @test m2.offset == 8

    # Captures and their offsets stay absolute (1-based) with a start offset.
    m3 = match(r"(\d+)", "abc123def456", 7)
    @test m3.match == "456"
    @test m3.offset == 10
    @test m3.captures[1] == "456"
    @test m3.offsets[1] == 10

    # No further match beyond `start` returns nothing.
    @test match(r"bc", "abcbc", 5) === nothing

    # Constructor result feeds straight into 3-arg match.
    @test match(Regex("(\\d+)"), "abc123def", 5).match == "23"
end

true
