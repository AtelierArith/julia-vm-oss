# Aggregated concat-safe @testset fixtures (Issue #9671 Phase 3 expansion).
# Each block below is one former standalone fixture, verbatim except its
# `using Test` / trailing `true` were hoisted. @testset names (with their
# original Issue numbers) are preserved, and the #9360 @testset gate still
# detects any per-@testset failure. Source fixture in each banner.
using Test

# ===== source: strings/chop_basic.jl =====
# Test chop function - remove characters from start/end of string


@testset "chop - default (remove last character)" begin
    @test chop("hello") == "hell"
    @test chop("a") == ""
    @test chop("") == ""
    @test chop("hello world") == "hello worl"
    @test chop("hello!") == "hello"
    @test chop("hello\n") == "hello"
end

@testset "chop with head and tail keywords (Issue #2045)" begin
    # head removes from start, tail removes from end
    @test chop("hello", head=2, tail=0) == "llo"
    @test chop("hello", head=0, tail=2) == "hel"
    @test chop("hello", head=1, tail=1) == "ell"

    # head=0, tail=0: no removal
    @test chop("hello", head=0, tail=0) == "hello"

    # Remove everything
    @test chop("ab", head=1, tail=1) == ""

    # Excess removal: returns empty
    @test chop("ab", head=5, tail=5) == ""

    # Only head
    @test chop("hello", head=3, tail=0) == "lo"

    # Only tail (different from default)
    @test chop("hello", head=0, tail=3) == "he"
end

# ===== source: strings/chopprefix_basic.jl =====
# Test: chopprefix function - remove prefix
# Expected: "world"


@testset "chopprefix(s, prefix) - remove prefix" begin

    @test (chopprefix("hello world", "hello ")) == "world"
end

# ===== source: strings/chopsuffix_basic.jl =====
# Test: chopsuffix function - remove suffix
# Expected: "hello"


@testset "chopsuffix(s, suffix) - remove suffix" begin

    @test (chopsuffix("hello world", " world")) == "hello"
end

# ===== source: strings/extended_unicode_char_escape_8870.jl =====
# Extended Unicode char escape literals (Issue #8870)


@testset "extended Unicode char escapes (Issue #8870)" begin
    @test ncodeunits('\u80') == 2
    @test ncodeunits('\U10000') == 4
    @test Int('\u80') == 0x80
    @test Int('\U10000') == 0x10000
end

# ===== source: strings/lpad_basic.jl =====
# Test: lpad function - left pad string
# Expected: "  abc" (5 chars total)


@testset "lpad(s, n) - left pad string" begin

    @test (lpad("abc", 5)) == "  abc"
end

# ===== source: strings/map_string.jl =====
# Test map(f, s::String) returns String (Issue #2609)

@testset "map on String returns String" begin
    # Basic: map uppercase over a string
    @test map(uppercase, "hello") == "HELLO"
    @test map(lowercase, "HELLO") == "hello"

    # Identity function
    @test map(identity, "abc") == "abc"

    # Lambda function
    @test map(c -> uppercase(c), "world") == "WORLD"

    # Empty string
    @test map(uppercase, "") == ""

    # Single character
    @test map(uppercase, "a") == "A"

    # Return type is String
    @test isa(map(uppercase, "hello"), String)
end

# ===== source: strings/regex_replace.jl =====
# replace(s, r"pattern" => new) with Regex patterns
# Issue #2112


@testset "replace with regex pattern" begin
    # Basic regex replace
    @test replace("hello world", r"world" => "julia") == "hello julia"

    # Replace all occurrences (default count=0)
    @test replace("aaa", r"a" => "b") == "bbb"

    # Replace with count limit
    @test replace("aaa", r"a" => "b", count=1) == "baa"
    @test replace("aaa", r"a" => "b", count=2) == "bba"

    # Pattern not found
    @test replace("hello", r"xyz" => "abc") == "hello"

    # Replace with empty string (deletion)
    @test replace("hello world", r" world" => "") == "hello"

    # 2-arg string replace still works
    @test replace("hello world", "world" => "julia") == "hello julia"
end

# ===== source: strings/reverse_string.jl =====
# reverse(::String) returns reversed String, not Vector{Char} (Issue #2053)


@testset "reverse(::String) returns String" begin
    @test reverse("hello") == "olleh"
    @test reverse("abc") == "cba"
    @test reverse("a") == "a"
    @test reverse("ab") == "ba"
    @test reverse("racecar") == "racecar"
end

@testset "reverse roundtrip" begin
    @test reverse(reverse("hello")) == "hello"
end

# ===== source: strings/rpad_basic.jl =====
# Test: rpad function - right pad string
# Expected: "abc  " (5 chars total)


@testset "rpad(s, n) - right pad string" begin

    @test (rpad("abc", 5)) == "abc  "
end

# ===== source: strings/string_case_escape.jl =====
# Test string functions: lowercasefirst, uppercasefirst, escape_string


@testset "lowercasefirst, uppercasefirst, escape_string" begin

    # === lowercasefirst(s) - convert first character to lowercase ===

    # Uppercase first character
    @assert lowercasefirst("Hello") == "hello"
    @assert lowercasefirst("WORLD") == "wORLD"
    @assert lowercasefirst("ABC") == "aBC"

    # Already lowercase first character
    @assert lowercasefirst("hello") == "hello"
    @assert lowercasefirst("world") == "world"

    # Single character
    @assert lowercasefirst("A") == "a"
    @assert lowercasefirst("a") == "a"

    # Empty string
    @assert lowercasefirst("") == ""

    # Non-letter first character
    @assert lowercasefirst("123abc") == "123abc"
    @assert lowercasefirst(" Hello") == " Hello"

    # === uppercasefirst(s) - convert first character to uppercase ===

    # Lowercase first character
    @assert uppercasefirst("hello") == "Hello"
    @assert uppercasefirst("world") == "World"
    @assert uppercasefirst("abc") == "Abc"

    # Already uppercase first character
    @assert uppercasefirst("Hello") == "Hello"
    @assert uppercasefirst("WORLD") == "WORLD"

    # Single character
    @assert uppercasefirst("a") == "A"
    @assert uppercasefirst("A") == "A"

    # Empty string
    @assert uppercasefirst("") == ""

    # Non-letter first character
    @assert uppercasefirst("123abc") == "123abc"
    @assert uppercasefirst(" hello") == " hello"

    # === escape_string(s) - escape special characters ===

    # Basic escaping
    @assert escape_string("hello") == "hello"
    @assert escape_string("world") == "world"

    # Backslash
    @assert escape_string("a\\b") == "a\\\\b"

    # Double quotes
    @assert escape_string("a\"b") == "a\\\"b"

    # Newline and tab
    @assert escape_string("a\nb") == "a\\nb"
    @assert escape_string("a\tb") == "a\\tb"

    # Carriage return
    @assert escape_string("a\rb") == "a\\rb"

    # Empty string
    @assert escape_string("") == ""

    # Multiple special characters
    @assert escape_string("a\n\tb") == "a\\n\\tb"

    # All tests passed
    @test (true)
end

# ===== source: strings/string_case_first_latin1.jl =====

# Regression tests for Issues #3608 and #3609:
# `uppercasefirst` and `lowercasefirst` must handle non-ASCII Latin-1
# letters (e.g. 'é' ↔ 'É', 'ü' ↔ 'Ü'). Previously they checked only the
# ASCII a-z / A-Z range (a single byte) and returned non-ASCII strings
# unchanged.

@testset "uppercasefirst Latin-1 (#3609)" begin
    @test uppercasefirst("élan") == "Élan"
    @test uppercasefirst("über") == "Über"
    @test uppercasefirst("ébc") == "Ébc"
    @test uppercasefirst("ñoño") == "Ñoño"
    @test uppercasefirst("ø") == "Ø"   # single Latin-1 char

    # ASCII regression — must still work
    @test uppercasefirst("hello") == "Hello"
    @test uppercasefirst("a") == "A"

    # Empty + already-uppercase + non-letter unchanged
    @test uppercasefirst("") == ""
    @test uppercasefirst("Hello") == "Hello"
    @test uppercasefirst("123") == "123"

    # Non-Latin-1 (CJK) returned unchanged — full Unicode case mapping is out
    # of scope; matches Julia behavior on chars without case.
    @test uppercasefirst("漢字") == "漢字"
end

@testset "lowercasefirst Latin-1 (#3608)" begin
    @test lowercasefirst("Élan") == "élan"
    @test lowercasefirst("ÉLAN") == "éLAN"
    @test lowercasefirst("Über") == "über"
    @test lowercasefirst("Ñoño") == "ñoño"
    @test lowercasefirst("Ø") == "ø"

    # ASCII regression
    @test lowercasefirst("Hello") == "hello"
    @test lowercasefirst("A") == "a"

    @test lowercasefirst("") == ""
    @test lowercasefirst("hello") == "hello"
    @test lowercasefirst("123") == "123"
end

# ===== source: strings/string_chopprefix_chopsuffix_non_ascii.jl =====

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

# ===== source: strings/string_escape_hex_unicode.jl =====
# Test string-literal hex/unicode/octal escape sequences (Issue #3569)


@testset "hex escape \\xNN in string literals" begin
    # Single-byte hex escape, ASCII range
    @test "\x41" == "A"
    @test "\x48\x69" == "Hi"
    @test "\x30" == "0"

    # Hex escape with one digit (greedy max 2 — single digit also works)
    @test "\x7" == "\a"

    # Hex escape stops after 2 digits even if more hex follow
    @test "\x41B" == "AB"
end

@testset "unicode escape \\uNNNN in string literals" begin
    # 4-digit unicode (ASCII range)
    @test "A" == "A"

    # 4-digit unicode (BMP)
    @test "é" == "é"

    # Greedy: 4 digits max
    @test "\u41A" == "К"   # 0x41A = Cyrillic capital El
end

@testset "unicode escape \\UNNNNNNNN in string literals" begin
    # 8-digit unicode
    @test "\U00000041" == "A"

    # Astral plane codepoint (emoji)
    @test "\U0001F600" == "😀"
end

@testset "octal escape \\NNN in string literals" begin
    # 3-digit octal
    @test "\101" == "A"   # 0o101 = 0x41 = 'A'

    # 1-digit octal
    @test "\7" == "\a"

    # Multi-octal sequence
    @test "\101\102" == "AB"
end

@testset "control character escapes in string literals" begin
    @test "\a" == "\x07"
    @test "\b" == "\x08"
    @test "\f" == "\x0c"
    @test "\v" == "\x0b"
    @test "\e" == "\x1b"
    @test "\0" == "\x00"
end

@testset "println of hex escape (regression for Issue #3569)" begin
    # The original bug: "\x41" was emitted literally as four bytes \x41
    s = "\x41"
    @test s == "A"
    @test length(s) == 1
end

# ===== source: strings/string_escape_non_ascii.jl =====

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

# ===== source: strings/string_replace_char.jl =====

# Regression test for Issue #3596:
# `replace(s, old => new)` must accept Char arguments on either side of the
# pair. Previously the impl called `length(old)` directly which failed with
# "length not defined for Char".

@testset "replace with Char pairs (#3596)" begin
    # Char => Char (MWE from issue)
    @test replace("aba", 'a' => 'x') == "xbx"

    # Char => String
    @test replace("aba", 'a' => "xx") == "xxbxx"

    # String => Char
    @test replace("aba", "a" => 'x') == "xbx"

    # All-Char on a longer string
    @test replace("hello world", 'l' => 'L') == "heLLo worLd"

    # No match for the Char
    @test replace("hello", 'z' => 'Z') == "hello"

    # Empty target string
    @test replace("", 'a' => 'b') == ""

    # Char with count keyword
    @test replace("aaaa", 'a' => 'b'; count=2) == "bbaa"

    # Existing String=>String still works (regression check)
    @test replace("hello", "ll" => "LL") == "heLLo"
end

# ===== source: strings/string_replace_non_ascii.jl =====

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

# ===== source: strings/string_strip_char.jl =====

# Regression test for Issue #3668:
# `strip`, `lstrip`, `rstrip` must accept a 2-arg `(s::String, c::Char)`
# form that strips occurrences of `c` from the appropriate end(s).
# Previously only the 1-arg whitespace form and predicate form existed.

@testset "strip(s, ::Char) (#3668)" begin
    @test strip("xxhelloxx", 'x') == "hello"
    @test strip("aabbccaa", 'a') == "bbcc"
    @test strip("xxhellox", 'x') == "hello"
    @test strip("xxx", 'x') == ""
    @test strip("", 'x') == ""
    @test strip("hello", 'x') == "hello"   # no match
    @test strip("hello", 'h') == "ello"    # only one end
end

@testset "lstrip(s, ::Char) (#3668)" begin
    @test lstrip("xxhello", 'x') == "hello"
    @test lstrip("xxhelloxx", 'x') == "helloxx"   # only left
    @test lstrip("hello", 'x') == "hello"
    @test lstrip("xxx", 'x') == ""
    @test lstrip("", 'x') == ""
end

@testset "rstrip(s, ::Char) (#3668)" begin
    @test rstrip("helloxx", 'x') == "hello"
    @test rstrip("xxhelloxx", 'x') == "xxhello"   # only right
    @test rstrip("hello", 'x') == "hello"
    @test rstrip("xxx", 'x') == ""
    @test rstrip("", 'x') == ""
end

@testset "1-arg whitespace strip (regression)" begin
    @test strip("  hello  ") == "hello"
    @test lstrip("  hello") == "hello"
    @test rstrip("hello  ") == "hello"
end

@testset "predicate-form strip (regression)" begin
    @test strip(c -> c == 'x', "xxhelloxx") == "hello"
    @test lstrip(c -> c == 'x', "xxhello") == "hello"
    @test rstrip(c -> c == 'x', "helloxx") == "hello"
    @test strip(isspace, " é ") == "é"
    @test strip(c -> c == 'x', "xéx") == "é"
    @test lstrip(c -> c == 'x', "xé") == "é"
    @test rstrip(c -> c == 'x', "éx") == "é"
    @test replace("éx", "x" => "y", "q" => "z") == "éy"
end

# ===== source: strings/string_textwidth.jl =====
# Test textwidth() function - get display width of string
# textwidth(s::String) -> Int64
# textwidth(c::Char) -> Int64


@testset "textwidth() - get display width of string/character" begin

    result = 0

    # Test ASCII string (each character has width 1)
    if textwidth("hello") == 5
        result = result + 1
    end

    # Test empty string
    if textwidth("") == 0
        result = result + 1
    end

    # Test single character
    if textwidth("A") == 1
        result = result + 1
    end

    # Test character function
    if textwidth('A') == 1
        result = result + 1
    end

    @test (result) == 4
end

# ===== source: strings/string_textwidth_unicode.jl =====

# Regression test for Issue #3598:
# `textwidth(s)` previously classified all non-ASCII characters as width 2.
# Latin-1 letters like 'é' should be width 1; only East Asian wide / fullwidth
# ranges are width 2.

@testset "textwidth Latin-1 (#3598)" begin
    @test textwidth("é") == 1
    @test textwidth('é') == 1
    @test textwidth("café") == 4
    @test textwidth("naïve") == 5
end

@testset "textwidth ASCII (regression)" begin
    @test textwidth("") == 0
    @test textwidth("hello") == 5
    @test textwidth("Hello, World!") == 13
    @test textwidth(' ') == 1
    @test textwidth('A') == 1
end

@testset "textwidth East Asian wide" begin
    # CJK Unified Ideographs
    @test textwidth("漢") == 2
    @test textwidth("漢字") == 4
    @test textwidth('漢') == 2
    # Mixed ASCII + CJK
    @test textwidth("a漢b") == 4
    @test textwidth("Hello, 世界") == 11

    # Hiragana / Katakana
    @test textwidth("あ") == 2
    @test textwidth('カ') == 2

    # Hangul Syllables
    @test textwidth("한") == 2
    @test textwidth("한국") == 4
end

@testset "textwidth control characters" begin
    # Control chars have zero width
    @test textwidth('\t') == 0
    @test textwidth('\n') == 0
    @test textwidth("\t\n") == 0
end

# ===== source: strings/string_thisind_reverseind.jl =====
# Test thisind and reverseind functions


@testset "thisind/reverseind - string index functions" begin

    # === thisind tests ===

    # ASCII string - every byte is a valid index
    s1 = "hello"
    @assert thisind(s1, 1) == 1
    @assert thisind(s1, 2) == 2
    @assert thisind(s1, 3) == 3
    @assert thisind(s1, 4) == 4
    @assert thisind(s1, 5) == 5

    # Edge cases
    @assert thisind(s1, 0) == 0        # Before start
    @assert thisind(s1, 6) == 6        # Past end (ncodeunits + 1)

    # Empty string
    @assert thisind("", 0) == 0
    @assert thisind("", 1) == 1

    # === reverseind tests ===

    # ASCII string - simple mapping
    s2 = "abc"
    # reverse("abc") = "cba"
    # reverseind(s, i) maps index in reverse(s) to index in s
    @assert reverseind(s2, 1) == 3  # 'c' at index 1 in reverse -> index 3 in original
    @assert reverseind(s2, 2) == 2  # 'b' at index 2 in reverse -> index 2 in original
    @assert reverseind(s2, 3) == 1  # 'a' at index 3 in reverse -> index 1 in original

    # Single character
    s3 = "x"
    @assert reverseind(s3, 1) == 1

    # Edge cases
    @assert reverseind(s2, 0) == 4  # Before start in reverse -> past end in original
    @assert reverseind(s2, 4) == 0  # Past end in reverse -> before start in original

    # Empty string
    @assert reverseind("", 0) == 1

    # All tests passed
    @test (true)
end

# ===== source: strings/string_unescape.jl =====
# Test unescape_string function (Issue #2086)


@testset "unescape_string(s) - unescape string escape sequences" begin

    # === Basic escape sequences ===

    # Newline
    @test unescape_string("hello\\nworld") == "hello\nworld"

    # Tab
    @test unescape_string("hello\\tworld") == "hello\tworld"

    # Carriage return
    @test unescape_string("hello\\rworld") == "hello\rworld"

    # Backslash
    @test unescape_string("hello\\\\world") == "hello\\world"

    # Quote
    @test unescape_string("hello\\\"world") == "hello\"world"

    # === No escape sequences ===
    @test unescape_string("hello world") == "hello world"
    @test unescape_string("") == ""
    @test unescape_string("abc") == "abc"

    # === Multiple escapes ===
    @test unescape_string("a\\nb\\tc") == "a\nb\tc"

    # === Hex escape ===
    @test unescape_string("\\x41") == "A"  # 0x41 = 'A'
    @test unescape_string("\\x48\\x69") == "Hi"
end

# ===== source: strings/strings_replace_array_5670.jl =====

# Issue #5670: `replace(collection, old => new, ...)` over an array replaces each
# ELEMENT matching a pair's first value (by `isequal`) with its second. sjulia
# only had the string `replace`, so the array form failed with NoMethodFound.

@testset "replace over an array substitutes matching elements (Issue #5670)" begin
    @test replace([1, 2, 3, 2], 2 => 20) == [1, 20, 3, 20]
    @test replace([1, 2, 3, 2], 2 => 20, 3 => 30) == [1, 20, 30, 20]
    @test replace([1, 2, 3], 5 => 50) == [1, 2, 3]          # no match
    @test replace(["a", "b", "a"], "a" => "X") == ["X", "b", "X"]

    # Matching is by equality, not predicate: no element equals the function.
    @test replace([1, 2, 3, 4], iseven => 0) == [1, 2, 3, 4]

    # The original array is not mutated.
    v = [1, 2, 3]
    @test replace(v, 2 => 99) == [1, 99, 3]
    @test v == [1, 2, 3]
end

@testset "string replace is unchanged (Issue #5670)" begin
    @test replace("hello", 'l' => 'L') == "heLLo"
    @test replace("aaa", "a" => "b") == "bbb"
    @test replace("hello world", "o" => "0", count=1) == "hell0 world"
end

# ===== source: strings/strip_basic.jl =====
# Test strip, lstrip, rstrip functions


@testset "strip, lstrip, rstrip - remove leading/trailing whitespace" begin

    # lstrip - remove leading whitespace
    @assert lstrip("  hello") == "hello"
    @assert lstrip("\t\nhello") == "hello"
    @assert lstrip("hello") == "hello"
    @assert lstrip("   ") == ""
    @assert lstrip("") == ""

    # rstrip - remove trailing whitespace
    @assert rstrip("hello  ") == "hello"
    @assert rstrip("hello\t\n") == "hello"
    @assert rstrip("hello") == "hello"
    @assert rstrip("   ") == ""
    @assert rstrip("") == ""

    # strip - remove both leading and trailing whitespace
    @assert strip("  hello  ") == "hello"
    @assert strip("\t\nhello\t\n") == "hello"
    @assert strip("hello") == "hello"
    @assert strip("   ") == ""
    @assert strip("") == ""

    # Mixed whitespace
    @assert strip("  hello world  ") == "hello world"
    @assert lstrip("  hello world  ") == "hello world  "
    @assert rstrip("  hello world  ") == "  hello world"

    @test (true)
end

# ===== source: strings/strip_predicate.jl =====
# lstrip/rstrip/strip with predicate function (Issue #2057, #2126)


@testset "lstrip with predicate" begin
    @test lstrip(isdigit, "123abc") == "abc"
    @test lstrip(isdigit, "abc") == "abc"
    @test lstrip(isdigit, "123") == ""
    @test lstrip(isspace, "  hello") == "hello"
end

@testset "rstrip with predicate" begin
    @test rstrip(isdigit, "abc123") == "abc"
    @test rstrip(isdigit, "abc") == "abc"
    @test rstrip(isdigit, "123") == ""
    @test rstrip(isspace, "hello  ") == "hello"
end

@testset "strip with predicate (Issue #2126)" begin
    @test strip(isdigit, "123abc456") == "abc"
    @test strip(isspace, "  hello  ") == "hello"
    @test strip(c -> c == 'x', "xxxhelloxxx") == "hello"
    @test strip(isdigit, "123") == ""
    @test strip(isdigit, "abc") == "abc"
    @test strip(isdigit, "") == ""
    @test strip(isspace, " ") == ""
    @test strip(isspace, "a") == "a"
    @test strip(isdigit, "123abc") == "abc"
    @test strip(isdigit, "abc456") == "abc"
end

# ===== source: strings/test_titlecase_edge_cases.jl =====
# titlecase edge cases - verify Pure Julia matches official Julia (Issue #2612)
# All assertions verified against: julia -e 'using Test; ...'


@testset "titlecase string edge cases" begin
    # Empty string
    @test titlecase("") == ""

    # Single character
    @test titlecase("a") == "A"
    @test titlecase("A") == "A"

    # Already titlecase
    @test titlecase("Hello World") == "Hello World"

    # All uppercase → titlecase (first letter upper, rest lower)
    @test titlecase("HELLO") == "Hello"
    @test titlecase("HELLO WORLD") == "Hello World"

    # All lowercase
    @test titlecase("hello world") == "Hello World"

    # Numbers and special characters (non-letter triggers next-letter capitalization)
    @test titlecase("hello123world") == "Hello123World"

    # Underscores and hyphens
    @test titlecase("hello_world") == "Hello_World"
    @test titlecase("hello-world") == "Hello-World"

    # Multiple spaces
    @test titlecase("hello  world") == "Hello  World"

    # Leading/trailing whitespace
    @test titlecase(" hello ") == " Hello "
end

# ===== source: strings/unescape_string_multibyte_6724.jl =====
# Issue #6724: unescape_string migrated from a Rust builtin to pure Julia
# (base/strings/util.jl). The previous Rust handler mixed byte/char indexing
# and corrupted multibyte input — e.g. unescape_string("café\\n") produced
# "cafÃ©\n" and dropped the trailing character. The pure-Julia version
# iterates over characters, so multibyte text is preserved while ASCII escape
# sequences are still decoded. Values verified against upstream julia 1.12.


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
