# Aggregated concat-safe @testset fixtures (Issue #9671 Phase 3 expansion).
# Each block below is one former standalone fixture, verbatim except its
# `using Test` / trailing `true` were hoisted. @testset names (with their
# original Issue numbers) are preserved, and the #9360 @testset gate still
# detects any per-@testset failure. Source fixture in each banner.
using Test

# ===== source: strings/invalid_utf8_string_8995.jl =====

@testset "String preserves invalid UTF-8 bytes (Issue #8995)" begin
    invalid = String(UInt8[0xff, 0x61])
    @test invalid isa String
    @test ncodeunits(invalid) == 2
    @test codeunit(invalid, 1) == 0xff
    @test codeunit(invalid, 2) == 0x61
    @test collect(codeunits(invalid)) == UInt8[0xff, 0x61]

    ascii = String(UInt8[0x61, 0x62])
    @test ascii == "ab"
    @test ncodeunits(ascii) == 2
    @test collect(codeunits(ascii)) == UInt8[0x61, 0x62]
end

# ===== source: strings/invalid_utf8_string_semantics_9589.jl =====

@testset "Invalid UTF-8 String keeps String semantics (Issue #9589)" begin
    s = String(UInt8[0xff, 0x61])

    f(x::String) = 1
    f(x) = 2
    a::Any = s
    @test f(a) == 1

    v = collect((s,))
    @test v isa Vector{String}
    @test length(v) == 1
    @test collect(codeunits(v[1])) == UInt8[0xff, 0x61]

    @test escape_string(s) == "\\xffa"
    @test collect(codeunits(escape_string(s))) == UInt8[0x5c, 0x78, 0x66, 0x66, 0x61]

    r = repr(s)
    @test r == "\"\\xffa\""
    @test collect(codeunits(r)) == UInt8[0x22, 0x5c, 0x78, 0x66, 0x66, 0x61, 0x22]
end

# ===== source: strings/isascii_string.jl =====
# isascii(::String) - check if all characters in string are ASCII (Issue #2046)


@testset "isascii for Char (existing)" begin
    @test isascii('a') == true
    @test isascii('A') == true
    @test isascii('0') == true
    @test isascii(' ') == true
end

@testset "isascii for String (Issue #2046)" begin
    @test isascii("hello") == true
    @test isascii("Hello World") == true
    @test isascii("abc123") == true
    @test isascii("") == true
    @test isascii("!@#\$%") == true
end

# ===== source: strings/regex_match_fields.jl =====
# Test RegexMatch field access (Issue #2116)


@testset "RegexMatch field access" begin
    m = match(r"(\d+)", "abc123def")
    @test m !== nothing

    # .match - the matched substring
    @test m.match == "123"

    # .offset - starting position (1-based)
    @test m.offset == 4

    # .captures - tuple of captured groups
    caps = m.captures
    @test caps[1] == "123"

    # .offsets - starting positions of each capture group
    offs = m.offsets
    @test offs[1] == 4
end

@testset "RegexMatch multiple captures" begin
    m = match(r"(\w+)@(\w+)\.(\w+)", "user@example.com")
    @test m !== nothing
    @test m.match == "user@example.com"
    @test m.captures[1] == "user"
    @test m.captures[2] == "example"
    @test m.captures[3] == "com"
    @test m.offset == 1
end

@testset "RegexMatch no match returns nothing" begin
    m = match(r"xyz", "abc")
    @test m === nothing
end

# ===== source: strings/string_ascii.jl =====
# Test ascii function - validate string contains only ASCII


@testset "ascii(s) - validate ASCII string" begin

    # === Valid ASCII strings ===
    @assert ascii("hello") == "hello"
    @assert ascii("WORLD") == "WORLD"
    @assert ascii("Hello World!") == "Hello World!"
    @assert ascii("12345") == "12345"
    @assert ascii("") == ""
    @assert ascii(" ") == " "
    @assert ascii("\t\n") == "\t\n"

    # ASCII characters (0-127)
    @assert ascii("ABC abc 123") == "ABC abc 123"
    @assert ascii("!@#\$%^&*()") == "!@#\$%^&*()"

    # All tests passed
    @test (true)
end

# ===== source: strings/string_char_concat.jl =====
# Test String * Char concatenation (Issue #2127)


@testset "String * Char concatenation" begin
    # Basic String * Char
    @test "Hello" * '!' == "Hello!"
    @test '>' * "arrow" == ">arrow"

    # Char * Char
    @test 'a' * 'b' == "ab"

    # typeof checks
    @test typeof("abc" * 'd') == String
    @test typeof('a' * "bc") == String
    @test typeof('a' * 'b') == String

    # Chained concatenation (n-ary reduction path)
    s = "Hello" * ' ' * "World"
    @test s == "Hello World"

    result = "a" * 'b' * "c" * 'd'
    @test result == "abcd"

    # Multi-step (to verify consistency)
    a = "Hello" * ' '
    r = a * "World"
    @test r == "Hello World"
end

# ===== source: strings/string_from_chars.jl =====
# String(::Vector{Char}) constructor - convert character array to string (Issue #2038)


@testset "String(collect(s)) round-trip" begin
    @test String(collect("hello")) == "hello"
    @test String(collect("world")) == "world"
    @test String(collect("")) == ""
    @test String(collect("abc def")) == "abc def"
end

@testset "String(char_array) from literal array" begin
    @test String(['a', 'b', 'c']) == "abc"
    @test String(['h', 'e', 'l', 'l', 'o']) == "hello"
end

@testset "String(char_array) reads logical reshaped elements (Issue #3908)" begin
    chars = collect("abcd")
    reshaped = reshape(chars, 4)
    chars[2] = 'Z'

    @test String(reshaped) == "aZcd"
end

@testset "String(s) identity for strings" begin
    @test String("hello") == "hello"
end

# ===== source: strings/string_index_vector_3908.jl =====

@testset "String index vector uses slice path (Issue #3908)" begin
    indices = Int64[]
    push!(indices, 1)
    push!(indices, 3)

    @test "abcd"[indices] == "ac"
    @test typeof("abcd"[indices]) == String
end

@testset "dynamic String index validation (Issue #11643)" begin
    dynamic_string_index_11643(s, i::Any) = s[i]

    @test dynamic_string_index_11643("abc", :) == "abc"
    @test_throws MethodError dynamic_string_index_11643("abc", [1.0])
    @test_throws MethodError dynamic_string_index_11643("abc", Any[1, "x"])
    @test_throws ArgumentError dynamic_string_index_11643("abc", Bool[true])
    @test_throws MethodError "abc"[1.0:1.0:3.0]
    @test_throws MethodError dynamic_string_index_11643("abc", 1.0:2.0:3.0)
end

# ===== source: strings/string_isletter_unicode.jl =====

# Regression tests for Issue #3601:
# `isletter`, `isuppercase`, `islowercase` previously checked only ASCII
# A-Z / a-z. Extended to Latin-1, Greek, Cyrillic, and main CJK ranges.

@testset "isletter Unicode (#3601)" begin
    # Latin-1 letters
    @test isletter('é') == true
    @test isletter('Ñ') == true
    @test isletter('ü') == true
    @test isletter('ø') == true

    # Greek
    @test isletter('α') == true
    @test isletter('Ω') == true

    # Cyrillic
    @test isletter('А') == true
    @test isletter('я') == true

    # CJK / Hangul / Hiragana
    @test isletter('漢') == true
    @test isletter('한') == true
    @test isletter('あ') == true

    # ASCII regression
    @test isletter('a') == true
    @test isletter('Z') == true

    # Non-letters
    @test isletter('1') == false
    @test isletter(' ') == false
    @test isletter('×') == false   # multiplication sign, not a letter
    @test isletter('!') == false
end

@testset "isuppercase Unicode" begin
    @test isuppercase('A') == true
    @test isuppercase('É') == true
    @test isuppercase('Ñ') == true
    @test isuppercase('Α') == true   # Greek capital alpha
    @test isuppercase('Ω') == true   # Greek capital omega
    @test isuppercase('А') == true   # Cyrillic capital A

    @test isuppercase('a') == false
    @test isuppercase('é') == false
    @test isuppercase('1') == false
end

@testset "islowercase Unicode" begin
    @test islowercase('a') == true
    @test islowercase('é') == true
    @test islowercase('ñ') == true
    @test islowercase('α') == true
    @test islowercase('ω') == true
    @test islowercase('а') == true   # Cyrillic small a
    @test islowercase('я') == true

    @test islowercase('A') == false
    @test islowercase('É') == false
    @test islowercase('1') == false
end

# ===== source: strings/string_isnumeric.jl =====
# Test isnumeric - Unicode numeric character check (Issue #6752)
#
# isnumeric is now Pure Julia (base/strings/unicode.jl): it binary-searches an
# embedded Nd/Nl/No codepoint range table generated from upstream julia's
# utf8proc, replacing the Rust `BuiltinId::Isnumeric` (`char::is_numeric()`).
# The previous fixture was ASCII-only and could not catch a regression in the
# non-ASCII Nd/Nl/No coverage; these cases are verified to match upstream julia.


@testset "isnumeric(c) - Unicode Nd/Nl/No (#6752)" begin
    # === ASCII digits / non-digits ===
    @test isnumeric('0')
    @test isnumeric('5')
    @test isnumeric('9')
    @test !isnumeric('a')
    @test !isnumeric('A')
    @test !isnumeric('Z')
    @test !isnumeric(' ')
    @test !isnumeric('!')

    # === Nd (decimal digit), non-ASCII ===
    @test isnumeric('٣')   # Arabic-Indic three (U+0663)
    @test isnumeric('۵')   # Extended Arabic-Indic five (U+06F5)
    @test isnumeric('৪')   # Bengali four (U+09EA)
    @test isnumeric('๓')   # Thai three (U+0E53)
    @test isnumeric('５')  # Fullwidth five (U+FF15)

    # === Nl (letter number) ===
    @test isnumeric('Ⅷ')   # Roman numeral eight (U+2167)
    @test isnumeric('ⅻ')   # Small roman numeral twelve (U+217B)

    # === No (other number) ===
    @test isnumeric('½')   # Vulgar fraction one half (U+00BD)
    @test isnumeric('¾')   # Vulgar fraction three quarters (U+00BE)
    @test isnumeric('⅓')   # Vulgar fraction one third (U+2153)
    @test isnumeric('③')   # Circled digit three (U+2462)
    @test isnumeric('①')   # Circled digit one (U+2460)

    # === Non-numeric non-ASCII (letters, NOT Nd/Nl/No) ===
    @test !isnumeric('万')  # CJK ideograph "ten thousand" (Lo)
    @test !isnumeric('α')   # Greek small letter alpha (Ll)
    @test !isnumeric('あ')  # Hiragana letter a (Lo)
    @test !isnumeric('語')  # CJK ideograph (Lo)

    # === As a higher-order predicate over a string ===
    @test count(isnumeric, "a1٣x½9万") == 4
end

# ===== source: strings/string_isvalid.jl =====
# Test isvalid function - check if index is valid character boundary


@testset "isvalid(s, i) - check if index is valid character boundary" begin

    # === ASCII string - all indices valid ===
    s1 = "hello"
    @assert isvalid(s1, 1)
    @assert isvalid(s1, 2)
    @assert isvalid(s1, 3)
    @assert isvalid(s1, 4)
    @assert isvalid(s1, 5)

    # === Out of bounds ===
    @assert !isvalid(s1, 0)
    @assert !isvalid(s1, 6)
    @assert !isvalid(s1, 10)

    # === Empty string ===
    @assert !isvalid("", 0)
    @assert !isvalid("", 1)

    # === Single character ===
    @assert isvalid("a", 1)
    @assert !isvalid("a", 0)
    @assert !isvalid("a", 2)

    # All tests passed
    @test (true)
end

# ===== source: strings/string_lastindex_bytes.jl =====

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

    standalone = String(UInt8[0x61, 0x80])
    @test lastindex(standalone) == 2
    @test collect(eachindex(standalone)) == [1, 2]
    @test thisind(standalone, 2) == 2
    @test isvalid(standalone, 2)

    malformed_sequence = String(UInt8[0xf0, 0x80, 0x80, 0x80])
    @test lastindex(malformed_sequence) == 1
    @test thisind(malformed_sequence, 4) == 1
end

# ===== source: strings/string_nextind_prevind.jl =====
# Test nextind and prevind functions - UTF-8 string index navigation


@testset "nextind/prevind - UTF-8 index navigation" begin

    # === nextind with ASCII strings ===
    s = "hello"
    @assert nextind(s, 0) == 1
    @assert nextind(s, 1) == 2
    @assert nextind(s, 2) == 3
    @assert nextind(s, 4) == 5
    @assert nextind(s, 5) == 6

    # === prevind with ASCII strings ===
    @assert prevind(s, 1) == 0
    @assert prevind(s, 2) == 1
    @assert prevind(s, 3) == 2
    @assert prevind(s, 5) == 4

    # === Edge cases ===
    @test_throws BoundsError prevind(s, 0)
    @test_throws BoundsError nextind(s, 6)

    # === Empty string ===
    empty = ""
    @assert nextind(empty, 0) == 1
    @test_throws BoundsError nextind(empty, 1)
    @assert prevind(empty, 1) == 0

    # All tests passed
    @test (true)
end

# ===== source: strings/string_union_vararg_mul_4350.jl =====

@testset "String/Char multiplication Union vararg dispatch" begin
    @test hasmethod(*, Tuple{String, Char})
    @test hasmethod(*, Tuple{String, Char, String})
    @test hasmethod(*, Tuple{Char, Char, String, Char})

    @test (*)("a", 'b', "c") == "abc"
    @test (*)('a', 'b', "cd", 'e') == "abcde"

    function join_parts(x::Union{String, Char}, ys::Union{String, Char}...)
        return string(x, ys...)
    end

    @test join_parts("x", 'y', "z") == "xyz"
    @test join_parts('x', "y", 'z', "!") == "xyz!"
end

# ===== source: strings/string_vector_show_quotes.jl =====
# Test that print(::Vector{T}) uses show-style quoting for elements
# (Issue #3574). Julia's `print(io, ::AbstractVector)` calls `show(io, x)`
# for each element, which adds quotes around String/Char.


@testset "String vector show quotes - Issue #3574" begin
    # Vector of String literals shows with quotes inline.
    @test sprint(print, ["a", "b"]) == "[\"a\", \"b\"]"
    @test sprint(print, ["foo", "bar", "baz"]) == "[\"foo\", \"bar\", \"baz\"]"

    # Vector of Char shows with single quotes.
    @test sprint(print, ['a', 'b']) == "['a', 'b']"

    # Numeric vectors are unaffected (no quotes).
    @test sprint(print, [1, 2, 3]) == "[1, 2, 3]"
    @test sprint(print, [1.0, 2.5]) == "[1.0, 2.5]"
end

# ===== source: strings/strings_join_three_arg_5663.jl =====

# Issue #5663: the 3-argument `join(itr, delim, last)` form uses a distinct
# separator `last` before the FINAL element — `join([1,2,3], ", ", " and ")` is
# "1, 2 and 3". sjulia only had the 1- and 2-argument `join` methods, so the
# 3-argument call failed with NoMethodFound.

@testset "join(itr, delim, last) uses a distinct final separator (Issue #5663)" begin
    @test join([1, 2, 3], ", ", " and ") == "1, 2 and 3"
    @test join([1, 2], ", ", " and ") == "1 and 2"
    @test join(["a", "b", "c", "d"], ", ", " or ") == "a, b, c or d"
    @test join(["x", "y"], "-", " & ") == "x & y"

    # Edge cases: single element ignores both separators; empty is "".
    @test join([42], ", ", " and ") == "42"
    @test join(String[], ", ", " and ") == ""

    # Works over a range, and the result is an ordinary String.
    @test join(1:3, ", ", " and ") == "1, 2 and 3"
    @test length(join([1, 2, 3], ", ", " and ")) == 10

    # The 1- and 2-argument forms are unchanged.
    @test join([1, 2, 3], ", ") == "1, 2, 3"
    @test join(["a", "b", "c"]) == "abc"
end

# ===== source: strings/strings_pure_dispatch_helpers.jl =====
# Verify Pure Julia dispatch for residual string helpers (Issue #3726)
#
# - isvalid(s::String, i::Integer) was previously dispatched to
#   BuiltinId::IsvalidIndex via compile/expr/builtin_string.rs.
# - findall(pattern::String, s::String) and findall(c::Char, s::String)
#   were previously routed to BuiltinId::StringFindAll in
#   compile/expr/call/mod.rs.
# - count(pattern::String, s::String) and count(c::Char, s::String) were
#   previously routed to BuiltinId::StringCount.
#
# After Issue #3726, all of the above resolve to Pure Julia methods in
# subset_julia_vm/src/julia/base/strings/{basic.jl,search.jl}. Calls fall
# through method dispatch and the Rust builtins remain only as cache-
# compatibility fallbacks (no longer reachable from new IR).


@testset "isvalid Pure Julia dispatch" begin
    # Multibyte UTF-8 character: 'é' = 2 codeunits (0xC3 0xA9)
    @test isvalid("é", 1) == true
    @test isvalid("é", 2) == false  # continuation byte
    # Out-of-bounds
    @test isvalid("é", 0) == false
    @test isvalid("é", 3) == false
    @test isvalid("abc", -1) == false
    @test isvalid("abc", 4) == false
    # ASCII string: every byte is a valid boundary
    @test isvalid("abc", 1) == true
    @test isvalid("abc", 2) == true
    @test isvalid("abc", 3) == true
    # Empty string: no valid indices
    @test isvalid("", 1) == false
end

@testset "findall(pattern::String, s::String) Pure Julia dispatch" begin
    # Non-overlapping matches
    r = findall("ana", "banana")
    @test length(r) == 1
    @test first(r[1]) == 2
    @test last(r[1]) == 4

    # Multiple non-overlapping matches
    r2 = findall("aba", "abababa")
    @test length(r2) == 2
    @test first(r2[1]) == 1
    @test last(r2[1]) == 3
    @test first(r2[2]) == 5
    @test last(r2[2]) == 7

    # No matches
    r3 = findall("xyz", "banana")
    @test length(r3) == 0

    # Single character pattern (still String overload)
    r4 = findall("a", "banana")
    @test length(r4) == 3
    @test first(r4[1]) == 2
    @test first(r4[2]) == 4
    @test first(r4[3]) == 6
end

@testset "findall(c::Char, s::String) Pure Julia dispatch" begin
    @test findall('a', "banana") == [2, 4, 6]
    @test isempty(findall('z', "banana"))
    @test findall('b', "banana") == [1]
end

@testset "count(pattern::String, s::String) Pure Julia dispatch" begin
    @test count("ana", "banana") == 1  # non-overlapping
    @test count("aba", "abababa") == 2
    @test count("xyz", "banana") == 0
    @test count("b", "abc") == 1
    @test count("abc", "abc") == 1
    # Empty pattern: count("", s) == length(s) + 1
    @test count("", "abc") == 4
    @test count("", "") == 1
end

@testset "count(c::Char, s::String) Pure Julia dispatch" begin
    @test count('a', "banana") == 3
    @test count('z', "banana") == 0
    @test count('l', "hello world") == 3
end

# ===== source: strings/test_string_exports.jl =====
# Test exported string functions


@testset "String manipulation functions" begin
    # chomp - remove trailing newline
    @test chomp("hello\n") == "hello"
    @test chomp("hello") == "hello"

    # chop - remove last character
    @test chop("hello") == "hell"

    # contains - check if substring exists
    @test contains("hello world", "world")
    @test !contains("hello", "xyz")

    # startswith/endswith
    @test startswith("hello", "he")
    @test endswith("hello", "lo")
    @test !startswith("hello", "lo")
    @test !endswith("hello", "he")

    # strip/lstrip/rstrip
    @test strip("  hello  ") == "hello"
    @test lstrip("  hello") == "hello"
    @test rstrip("hello  ") == "hello"

    # join
    @test join(["a", "b", "c"], ", ") == "a, b, c"
    @test join(["x"], "-") == "x"

    # occursin
    @test occursin("ell", "hello")
    @test !occursin("xyz", "hello")

    # uppercasefirst/lowercasefirst
    @test uppercasefirst("hello") == "Hello"
    @test lowercasefirst("Hello") == "hello"

    # replace with Pair
    @test replace("hello world", "world" => "Julia") == "hello Julia"

    # escape_string
    @test escape_string("a\nb") == "a\\nb"
end

true
