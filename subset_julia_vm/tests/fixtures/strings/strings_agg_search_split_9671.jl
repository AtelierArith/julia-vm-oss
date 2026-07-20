# Aggregated concat-safe @testset fixtures (Issue #9671 Phase 3 expansion).
# Each block below is one former standalone fixture, verbatim except its
# `using Test` / trailing `true` were hoisted. @testset names (with their
# original Issue numbers) are preserved, and the #9360 @testset gate still
# detects any per-@testset failure. Source fixture in each banner.
using Test

# ===== source: strings/count_predicate_string.jl =====
# count(predicate, string) - count characters satisfying predicate (Issue #2078)
# In Julia, count(f, itr) works for any iterable including strings.


@testset "count(predicate, string) (Issue #2078)" begin
    # Character classification predicates
    @test count(isletter, "h3ll0") == 3
    @test count(isdigit, "abc123") == 3
    @test count(isspace, "hello world") == 1
    @test count(isuppercase, "Hello World") == 2

    # Lambda predicates
    @test count(x -> x == 'l', "hello") == 2
    @test count(c -> c == 'a', "banana") == 3

    # Edge cases
    @test count(isletter, "") == 0
    @test count(isletter, "123") == 0
    @test count(isdigit, "abc") == 0

    # Regression: count(f, array) still works
    @test count(x -> x > 3, [1, 2, 3, 4, 5]) == 2
    @test count(isodd, [1, 2, 3, 4, 5]) == 3

    # Regression: count(pattern, string) still works
    @test count("ab", "ababab") == 3
end

# ===== source: strings/curried_string_search.jl =====
# Curried string search functions: startswith, endswith, contains, occursin
# Issue #2100


@testset "startswith curried" begin
    f = startswith("he")
    @test f("hello") == true
    @test f("world") == false
    @test f("help") == true
    @test f("") == false
    # 2-arg form still works
    @test startswith("hello", "he") == true
    @test startswith("hello", "wo") == false
end

@testset "endswith curried" begin
    f = endswith("lo")
    @test f("hello") == true
    @test f("world") == false
    @test f("polo") == true
    @test f("") == false
    # 2-arg form still works
    @test endswith("hello", "lo") == true
    @test endswith("hello", "he") == false
end

@testset "contains curried" begin
    f = contains("world")
    @test f("hello world") == true
    @test f("hello") == false
    @test f("worldwide") == true
    # 2-arg form still works
    @test contains("hello world", "world") == true
    @test contains("hello", "xyz") == false
end

@testset "occursin curried" begin
    f = occursin("hello world")
    @test f("world") == true
    @test f("xyz") == false
    @test f("hello") == true
    # 2-arg form still works
    @test occursin("world", "hello world") == true
    @test occursin("xyz", "hello") == false
end

# ===== source: strings/filter_string.jl =====
# filter(pred, s::String) returns String, not Vector{Char} (Issue #2062)


@testset "filter(::Function, ::String) returns String" begin
    @test filter(isletter, "h3ll0 w0rld") == "hllwrld"
    @test filter(isdigit, "h3ll0 w0rld") == "300"
    @test filter(isspace, "hello world") == " "
    @test filter(isletter, "123") == ""
    @test filter(isletter, "") == ""
    @test filter(isletter, "abc") == "abc"
end

# ===== source: strings/findfirst_findlast_char.jl =====
# findfirst/findlast with Char and String patterns (Issue #2030)
# Char pattern returns Int64 index, String pattern returns UnitRange{Int64}


@testset "findfirst/findlast char and string patterns (Issue #2030)" begin
    # findfirst with Char pattern - returns Int64
    @test findfirst('l', "hello") == 3
    @test findfirst('h', "hello") == 1
    @test findfirst('o', "hello") == 5
    @test findfirst('x', "hello") === nothing

    # findlast with Char pattern - returns Int64
    @test findlast('l', "hello") == 4
    @test findlast('h', "hello") == 1
    @test findlast('x', "hello") === nothing

    # findfirst with String pattern - returns UnitRange
    r1 = findfirst("ll", "hello")
    @test first(r1) == 3
    @test last(r1) == 4

    r2 = findfirst("he", "hello")
    @test first(r2) == 1
    @test last(r2) == 2

    @test findfirst("xx", "hello") === nothing

    # findlast with String pattern - returns UnitRange
    r3 = findlast("ll", "hello llama")
    @test first(r3) == 7
    @test last(r3) == 8

    r4 = findlast("he", "hello")
    @test first(r4) == 1
    @test last(r4) == 2

    @test findlast("xx", "hello") === nothing

    # Single character String pattern - UnitRange with same start/end
    r5 = findfirst("l", "hello")
    @test first(r5) == 3
    @test last(r5) == 3

    # Predicate form still works
    @test findfirst(x -> x > 3, [1, 2, 5, 4]) == 3
end

# ===== source: strings/first_last_string.jl =====
# first() and last() for strings (Issue #2048)


@testset "first(::String) - get first character" begin
    @test first("hello") == 'h'
    @test first("A") == 'A'
    @test first("123") == '1'
end

@testset "last(::String) - get last character" begin
    @test last("hello") == 'o'
    @test last("A") == 'A'
    @test last("123") == '3'
end

@testset "first(::String, n) - get first n characters" begin
    @test first("hello", 3) == "hel"
    @test first("hello", 1) == "h"
    @test first("hello", 5) == "hello"
end

@testset "last(::String, n) - get last n characters" begin
    @test last("hello", 3) == "llo"
    @test last("hello", 1) == "o"
    @test last("hello", 5) == "hello"
end

# ===== source: strings/occursin_basic.jl =====
# Test occursin - check if needle appears in haystack


@testset "occursin(needle, haystack) - Pure Julia (Issue #681)" begin
    @test (occursin("world", "hello world") && occursin("", "abc") && !occursin("xyz", "abc") && !occursin("abcd", "abc"))
end

# ===== source: strings/string_eachrsplit.jl =====
# Test eachrsplit function - reverse string split iterator (Issue #1994)
# eachrsplit yields substrings from right to left.


@testset "eachrsplit basic" begin
    # Basic right-to-left split
    @test collect(eachrsplit("a.b.c", ".")) == ["c", "b", "a"]
    @test collect(eachrsplit("hello::world::test", "::")) == ["test", "world", "hello"]

    # No delimiter found - returns whole string
    @test collect(eachrsplit("hello", ",")) == ["hello"]

    # Single element
    @test collect(eachrsplit("one", ".")) == ["one"]
end

@testset "eachrsplit with Char delimiter" begin
    @test collect(eachrsplit("x-y-z", '-')) == ["z", "y", "x"]
    @test collect(eachrsplit("a,b,c,d", ',')) == ["d", "c", "b", "a"]
end

@testset "eachrsplit reverse of eachsplit" begin
    # eachrsplit yields in reverse order compared to eachsplit
    s = "one.two.three"
    forward = collect(eachsplit(s, "."))
    backward = collect(eachrsplit(s, "."))
    @test length(forward) == length(backward)
    @test isequal(forward[1], backward[3])
    @test isequal(forward[2], backward[2])
    @test isequal(forward[3], backward[1])
end

# ===== source: strings/string_findnext_findprev.jl =====
# Test findnext and findprev string search functions


@testset "findnext/findprev - find next/previous occurrence in string" begin

    # findnext with character (returns Int64 or nothing)
    result1 = findnext('a', "abcabc", 1)
    @test result1 == 1

    result2 = findnext('a', "abcabc", 2)
    @test result2 == 4

    result3 = findnext('a', "abcabc", 5)
    @test result3 === nothing

    result4 = findnext('z', "abcabc", 1)
    @test result4 === nothing

    # findnext with substring (returns UnitRange{Int64} or nothing)
    result5 = findnext("bc", "abcabc", 1)
    @test first(result5) == 2
    @test last(result5) == 3

    result6 = findnext("bc", "abcabc", 3)
    @test first(result6) == 5
    @test last(result6) == 6

    result7 = findnext("xyz", "abcabc", 1)
    @test result7 === nothing

    # findprev with character (returns Int64 or nothing)
    result8 = findprev('a', "abcabc", 6)
    @test result8 == 4

    result9 = findprev('a', "abcabc", 3)
    @test result9 == 1

    result10 = findprev('z', "abcabc", 6)
    @test result10 === nothing

    # findprev with substring (returns UnitRange{Int64} or nothing)
    result11 = findprev("ab", "abcabc", 6)
    @test first(result11) == 4
    @test last(result11) == 5

    result12 = findprev("ab", "abcabc", 3)
    @test first(result12) == 1
    @test last(result12) == 2
end

# ===== source: strings/string_occursin_char.jl =====
# Test occursin with Char needle (Issue #3570)
# Julia: occursin(c::AbstractChar, s::AbstractString) = any(==(c), s)


@testset "occursin(c::Char, s::String) - Issue #3570" begin
    # Present
    @test occursin('o', "foo") == true
    @test occursin('f', "foo") == true
    @test occursin(' ', "a b") == true
    @test occursin('\n', "a\nb") == true
    @test occursin('\t', "a\tb") == true

    # Absent
    @test occursin('z', "foo") == false
    @test occursin('a', "") == false
    @test occursin('A', "abc") == false
end

# ===== source: strings/string_rsplit_keyword_limit.jl =====

# Regression test for Issue #3610:
# `rsplit(s, delim; limit=N)` keyword form must thread `limit` through to the
# positional impl. Previously the keyword was silently dropped and the full
# split was returned.

@testset "rsplit keyword limit (#3610)" begin
    # Basic case from the Issue MWE
    @test rsplit("a,b,c", ","; limit=2) == ["a,b", "c"]

    # Char delimiter
    @test rsplit("a,b,c", ','; limit=2) == ["a,b", "c"]

    # Larger limit keeps more rightmost splits
    @test rsplit("a,b,c,d", ","; limit=3) == ["a,b", "c", "d"]
    @test rsplit("a,b,c,d", ","; limit=2) == ["a,b,c", "d"]

    # limit=1 keeps the whole string as one part
    @test rsplit("a,b,c", ","; limit=1) == ["a,b,c"]

    # limit=0 means no limit (Julia default)
    @test rsplit("a,b,c", ","; limit=0) == ["a", "b", "c"]

    # Default keyword (no limit specified) matches limit=0
    @test rsplit("a,b,c", ",") == ["a", "b", "c"]
end

# ===== source: strings/string_search_non_ascii.jl =====

# Regression tests for Issues #3602, #3603, #3604:
# `startswith`, `endswith`, `occursin` previously mixed `length` (char count)
# with `codeunit` (byte index), producing false positives for non-ASCII
# inputs sharing leading/trailing UTF-8 bytes. Now uses `ncodeunits`.

@testset "startswith non-ASCII (#3602)" begin
    # MWE: distinct one-character non-ASCII prefixes sharing leading byte
    @test startswith("ê", "é") == false
    @test startswith("éx", "é") == true
    @test startswith("éxy", "ê") == false
    @test startswith("漢字", "漢") == true
    @test startswith("世界", "漢") == false

    # ASCII regression
    @test startswith("hello", "he") == true
    @test startswith("hello", "lo") == false
    @test startswith("hello", "") == true
    @test startswith("", "x") == false
    @test startswith("", "") == true
end

@testset "endswith non-ASCII (#3603)" begin
    # MWE
    @test endswith("ê", "é") == false
    @test endswith("xé", "é") == true
    @test endswith("xê", "é") == false
    @test endswith("漢字", "字") == true
    @test endswith("漢字", "漢") == false

    # ASCII regression
    @test endswith("hello", "lo") == true
    @test endswith("hello", "ho") == false
    @test endswith("hello", "") == true
end

@testset "occursin non-ASCII (#3604)" begin
    # MWE
    @test occursin("é", "ê") == false
    @test occursin("é", "xéy") == true
    @test occursin("é", "ééé") == true
    @test occursin("漢", "中漢字") == true
    @test occursin("漢", "字字字") == false

    # Substring of multi-char non-ASCII
    @test occursin("café", "Le café est chaud") == true
    @test occursin("kafe", "Le café est chaud") == false

    # ASCII regression
    @test occursin("ll", "hello") == true
    @test occursin("xy", "hello") == false
    @test occursin("", "hello") == true
end

# ===== source: strings/string_split.jl =====
# Test string split function
# Based on Julia's base/strings/util.jl


@testset "String split function" begin
    # Basic split with string delimiter
    @test split("a,b,c", ",") == ["a", "b", "c"]
    @test split("hello world", " ") == ["hello", "world"]

    # Split with multi-character delimiter
    @test split("a::b::c", "::") == ["a", "b", "c"]

    # Split with no delimiter matches
    @test split("hello", ",") == ["hello"]

    # Split at beginning
    @test split(",a,b", ",") == ["", "a", "b"]

    # Split at end
    @test split("a,b,", ",") == ["a", "b", ""]

    # Multiple consecutive delimiters
    @test split("a,,b", ",") == ["a", "", "b"]

    # Empty string
    @test split("", ",") == [""]

    # Single character split
    @test split("abc", "") == ["a", "b", "c"]

    # Split with Char delimiter
    @test split("a-b-c", '-') == ["a", "b", "c"]
end

@testset "String split with limit keyword (Issue #2040)" begin
    # limit=2: split at most once
    @test split("a-b-c-d", "-", limit=2) == ["a", "b-c-d"]

    # limit=3: split at most twice
    @test split("a-b-c-d", "-", limit=3) == ["a", "b", "c-d"]

    # limit=1: no split at all
    @test split("a-b-c-d", "-", limit=1) == ["a-b-c-d"]

    # limit=0: no limit (same as default)
    @test split("a-b-c-d", "-", limit=0) == ["a", "b", "c", "d"]

    # limit with space delimiter
    @test split("hello world foo bar", " ", limit=2) == ["hello", "world foo bar"]

    # limit with Char delimiter
    @test split("a-b-c-d", '-', limit=2) == ["a", "b-c-d"]

    # limit greater than number of parts: returns all parts
    @test split("a-b", "-", limit=10) == ["a", "b"]
end

# ===== source: strings/string_split_empty_utf8.jl =====
# Test split(s, "") on non-ASCII strings (Issue #3597)
# Empty delimiter should split by character, not by UTF-8 byte.


@testset "split(s, \"\") preserves multi-byte characters" begin
    # 2-byte UTF-8 characters (Latin-1 supplement)
    @test split("éa", "") == ["é", "a"]
    @test split("aé", "") == ["a", "é"]
    @test split("éé", "") == ["é", "é"]

    # 2-byte Greek
    @test split("αβγ", "") == ["α", "β", "γ"]

    # 3-byte CJK
    @test split("日本語", "") == ["日", "本", "語"]

    # 4-byte emoji (supplementary plane)
    @test split("a😀b", "") == ["a", "😀", "b"]

    # ASCII still correct
    @test split("abc", "") == ["a", "b", "c"]

    # Limit interacts correctly with multi-byte chars
    @test split("éaβ", ""; limit=2) == ["é", "aβ"]
    @test split("éaβ", ""; limit=1) == ["éaβ"]

    # Each output element has length 1 (one character)
    parts = split("éa", "")
    @test length(parts) == 2
    @test length(parts[1]) == 1
    @test length(parts[2]) == 1
end

# ===== source: strings/string_split_keepempty.jl =====

# Regression test for Issue #3651:
# split(s, delim; keepempty=...) and rsplit(s, delim; keepempty=...) must
# both honor the keyword. Previously the keyword was silently dropped and
# both forms returned all parts including empties.

@testset "split keepempty (#3651)" begin
    # default keepempty=true: keep "" between consecutive delims and at ends
    @test split(",a,,b,", ",") == ["", "a", "", "b", ""]
    @test split(",a,,b,", ","; keepempty=true) == ["", "a", "", "b", ""]
    # keepempty=false: drop empties
    @test split(",a,,b,", ","; keepempty=false) == ["a", "b"]
    # combined with limit (limit applies first; then keepempty=false filters)
    @test split(",a,,b,c", ","; limit=3, keepempty=false) == ["a", "b", "c"]
    # Char delimiter
    @test split(",a,,b,", ','; keepempty=false) == ["a", "b"]
    # No empties to drop — keepempty=false is no-op
    @test split("a,b,c", ","; keepempty=false) == ["a", "b", "c"]
end

@testset "rsplit keepempty (#3651)" begin
    # default keepempty=true
    @test rsplit(",a,,b,", ",") == ["", "a", "", "b", ""]
    @test rsplit(",a,,b,", ","; keepempty=true) == ["", "a", "", "b", ""]
    # keepempty=false
    @test rsplit(",a,,b,", ","; keepempty=false) == ["a", "b"]
    # combined with limit (rsplit limit splits from the right; then filter empties)
    @test rsplit(",a,,b,", ","; limit=3, keepempty=false) == ["a", "b"]
    # Char delimiter
    @test rsplit(",a,,b,", ','; keepempty=false) == ["a", "b"]
end

# ===== source: strings/string_split_whitespace.jl =====
# Test split(s::String) with no separator — whitespace default (Issue #3571)
# Julia: split(s::AbstractString; limit=0, keepempty=false) =
#         split(s, isspace; limit, keepempty)


@testset "split(s::String) whitespace default - Issue #3571" begin
    # Basic single-space separation
    @test split("a b c") == ["a", "b", "c"]
    @test split("a") == ["a"]

    # Multiple consecutive whitespace must collapse, leading/trailing trimmed
    @test split("  hi  there  ") == ["hi", "there"]
    @test split("   leading") == ["leading"]
    @test split("trailing   ") == ["trailing"]

    # Empty / all-whitespace inputs return an empty array
    # (Compare via isempty/length to avoid a separate VM limitation
    # where `Vector{String}() == Vector{String}()` lacks a method.)
    @test isempty(split(""))
    @test isempty(split("   "))
    @test isempty(split("\t\n "))

    # Mixed whitespace: tabs, newlines, spaces
    @test split("a\tb\nc") == ["a", "b", "c"]
    @test split("\ta\n b\rc") == ["a", "b", "c"]
end

# ===== source: strings/strings_startswith_regex_5676.jl =====

# Issue #5676: `startswith(s, re::Regex)` — true iff the regex matches at the
# START of the string. sjulia only handled string prefixes ("expected String,
# got Regex"). Implemented as: the regex's leftmost match (from `match`) begins at
# index 1, equivalent to upstream's start-anchored match.

@testset "startswith with a Regex pattern (Issue #5676)" begin
    @test startswith("hello", r"h.") == true
    @test startswith("hello", r"x") == false
    @test startswith("hello", r"l") == false      # matches at 3, not the start
    @test startswith("hello", r".*o") == true
    @test startswith("hello", r"he|xy") == true    # alternation, matches at start
    @test startswith("abc", r"[a-c]") == true
    @test startswith("123abc", r"\d+") == true
    @test startswith("abc123", r"\d+") == false
end

@testset "string startswith is unchanged (Issue #5676)" begin
    @test startswith("hello", "he") == true
    @test startswith("hello", "x") == false
    @test startswith("hello", "") == true
end

# ===== source: strings/test_occursin_hybrid_dispatch.jl =====
# Test hybrid dispatch for occursin: String path (Pure Julia) + Regex path (Rust builtin)
# Issue #2614: Verify both dispatch paths work correctly


@testset "occursin hybrid dispatch (Issue #2614)" begin
    @testset "String needle (Pure Julia path)" begin
        @test occursin("world", "hello world") == true
        @test occursin("xyz", "hello world") == false
        @test occursin("", "hello") == true
        @test occursin("hello", "hello") == true
        @test occursin("hello!", "hello") == false
    end

    @testset "Regex needle (Rust builtin path)" begin
        @test occursin(r"world", "hello world") == true
        @test occursin(r"^hello", "hello world") == true
        @test occursin(r"xyz", "hello world") == false
        @test occursin(r"\d+", "abc123") == true
        @test occursin(r"\d+", "abcdef") == false
    end

    @testset "Both paths produce consistent results" begin
        # Same substring test via both paths
        s = "The quick brown fox"
        @test occursin("quick", s) == occursin(r"quick", s)
        @test occursin("slow", s) == occursin(r"slow", s)
        @test occursin("fox", s) == occursin(r"fox", s)
    end
end

true
