# Aggregated concat-safe @testset fixtures (Issue #9671 Phase 3 expansion).
# Each block below is one former standalone fixture, verbatim except its
# `using Test` / trailing `true` were hoisted. @testset names (with their
# original Issue numbers) are preserved, and the #9360 @testset gate still
# detects any per-@testset failure. Source fixture in each banner.
using Test

# ===== source: strings/char_arithmetic.jl =====
# Char arithmetic and Char↔Int conversion (Issue #2035)
# Char values can be converted to/from integers and used in arithmetic expressions.


@testset "Char arithmetic and conversion (Issue #2035)" begin
    # Int(char) - char to codepoint
    @test Int('A') == 65
    @test Int('a') == 97
    @test Int('0') == 48
    @test Int(' ') == 32

    # Char(n) - codepoint to char
    @test Char(65) == 'A'
    @test Char(97) == 'a'
    @test Char(48) == '0'

    # Char + Int arithmetic → Char (Issue #2122)
    @test ('a' + 1) == 'b'    # 'a' + 1 = 'b'
    @test ('A' + 32) == 'a'   # 'A' + 32 = 'a' (lowercase)
    @test ('0' + 5) == '5'    # '0' + 5 = '5'

    # Int + Char arithmetic → Char (commutative, Issue #2122)
    @test (1 + 'a') == 'b'
    @test (32 + 'A') == 'a'

    # Char - Char arithmetic → Int (difference of codepoints)
    @test ('z' - 'a') == 25
    @test ('Z' - 'A') == 25
    @test ('9' - '0') == 9
    @test ('b' - 'a') == 1

    # Char - Int arithmetic → Char (Issue #2122)
    @test ('z' - 1) == 'y'    # 'z' - 1 = 'y'
    @test ('b' - 1) == 'a'    # 'b' - 1 = 'a'

    # Char comparison (returns Bool)
    @test ('a' < 'z') == true
    @test ('A' < 'a') == true
    @test ('a' == 'a') == true
    @test ('a' != 'b') == true

    # Roundtrip: Int→Char→Int
    @test Int(Char(42)) == 42
    @test Int(Char(Int('X'))) == Int('X')
end

# ===== source: strings/char_codepoint_bounds.jl =====

# Bug fix: Char(n) must throw for n > 0x10FFFF instead of wrapping (Issue #3457)

@testset "strings_char_codepoint_bounds_valid" begin
    @test Char(0) == '\0'
    @test Char(65) == 'A'
    @test Char(0x10FFFF) == Char(1114111)
end

@testset "strings_char_codepoint_bounds_negative (Issue #3457)" begin
    @test_throws InexactError Char(-1)
end

@testset "strings_char_codepoint_bounds_above_u32_max (Issue #3457)" begin
    @test_throws InexactError Char(4294967296)
end

# ===== source: strings/char_predicates.jl =====
# Test character classification predicates (Issue #1885)


@testset "isdigit" begin
    @test isdigit('0') == true
    @test isdigit('5') == true
    @test isdigit('9') == true
    @test isdigit('a') == false
    @test isdigit('Z') == false
    @test isdigit(' ') == false
end

@testset "isletter" begin
    @test isletter('a') == true
    @test isletter('z') == true
    @test isletter('A') == true
    @test isletter('Z') == true
    @test isletter('0') == false
    @test isletter(' ') == false
end

@testset "isuppercase" begin
    @test isuppercase('A') == true
    @test isuppercase('Z') == true
    @test isuppercase('a') == false
    @test isuppercase('z') == false
    @test isuppercase('0') == false
end

@testset "islowercase" begin
    @test islowercase('a') == true
    @test islowercase('z') == true
    @test islowercase('A') == false
    @test islowercase('Z') == false
    @test islowercase('0') == false
end

@testset "isascii" begin
    @test isascii('A') == true
    @test isascii('0') == true
    @test isascii(' ') == true
end

@testset "isspace" begin
    @test isspace(' ') == true
    @test isspace('A') == false
    @test isspace('0') == false
end

@testset "isprint" begin
    @test isprint('A') == true
    @test isprint(' ') == true
    @test isprint('0') == true
    @test isprint('~') == true
end

# ===== source: strings/codepoint_bitstring_pure_julia_6747.jl =====
# Issue #6747: codepoint and bitstring are now pure Julia. codepoint(c) = the
# Unicode codepoint as UInt32 (base/strings/basic.jl); bitstring(x) builds the
# binary representation from the bits via reinterpret-to-unsigned
# (base/intfuncs.jl). The raw-byte-access primitives ncodeunits / codeunit stay
# as byte primitives; codeunits and Char(n)/Int(c) are now Pure Julia wrappers
# over internal storage boundaries. Values verified against upstream julia 1.12.


@testset "codepoint is pure Julia, returns UInt32 (Issue #6747)" begin
    @test codepoint('A') === UInt32(65)
    @test codepoint('z') === UInt32(122)
    @test codepoint('0') === UInt32(48)
    @test codepoint('é') === UInt32(0x00e9)
    @test map(codepoint, ['a', 'b']) == UInt32[97, 98]
end

@testset "bitstring is pure Julia, matches upstream (Issue #6747)" begin
    @test bitstring(Int32(4)) == "00000000000000000000000000000100"
    @test bitstring(UInt8(5)) == "00000101"
    @test bitstring(Int8(-1)) == "11111111"
    @test bitstring(UInt16(0xABCD)) == "1010101111001101"
    @test bitstring(Int64(-1)) == "1111111111111111111111111111111111111111111111111111111111111111"
    @test bitstring(1.0f0) == "00111111100000000000000000000000"
    @test bitstring(2.2) == "0100000000000001100110011001100110011001100110011001100110011010"
    @test bitstring(true) == "00000001"
    @test bitstring(Float16(1.0)) == "0011110000000000"
end

@testset "byte-access primitives still work (Issue #6747)" begin
    @test ncodeunits("abc") == 3
    @test codeunit("abc", 1) == 0x61
    @test Char(65) === 'A'
    @test Int('A') === 65
end

# ===== source: strings/codeunits.jl =====
# Test codeunits function - returns a CodeUnits wrapper over UTF-8 bytes


@testset "codeunits - get string byte CodeUnits" begin

    # Basic ASCII string
    s = "Hello"
    cu = codeunits(s)
    @assert length(cu) == 5
    @assert Int64(cu[1]) == 72   # 'H' = 0x48
    @assert Int64(cu[2]) == 101  # 'e' = 0x65
    @assert Int64(cu[3]) == 108  # 'l' = 0x6c
    @assert Int64(cu[4]) == 108  # 'l' = 0x6c
    @assert Int64(cu[5]) == 111  # 'o' = 0x6f

    # Empty string
    s2 = ""
    cu2 = codeunits(s2)
    @assert length(cu2) == 0

    # Single character
    s3 = "A"
    cu3 = codeunits(s3)
    @assert length(cu3) == 1
    @assert Int64(cu3[1]) == 65  # 'A' = 0x41

    # "Hi" has bytes 72 ('H') and 105 ('i')
    hi_cu = codeunits("Hi")
    @assert Int64(hi_cu[1]) == 72
    @assert Int64(hi_cu[2]) == 105

    @test (true)
end

# ===== source: strings/repeat_char.jl =====
# repeat(c::Char, n) returns String of repeated character (Issue #2057)


@testset "repeat(::Char, ::Int) basic" begin
    @test repeat('a', 5) == "aaaaa"
    @test repeat('-', 3) == "---"
    @test repeat('x', 1) == "x"
    @test repeat('z', 0) == ""
end

@testset "repeat(::Char, ::Int) special chars" begin
    @test repeat(' ', 4) == "    "
    @test repeat('0', 3) == "000"
end

# ===== source: strings/string_codepoint.jl =====
# Test codepoint() function - get Unicode code point of character
# codepoint(c::Char) -> UInt32


@testset "codepoint() - get Unicode code point of character" begin
    # Test ASCII characters
    @test codepoint('A') == 65
    @test codepoint('a') == 97
    @test codepoint('0') == 48
    @test codepoint(' ') == 32
    @test codepoint('Z') == 90
    @test codepoint('z') == 122

    # Test return type is UInt32
    @test typeof(codepoint('A')) == UInt32
end

# ===== source: strings/titlecase_char.jl =====
# titlecase(c::Char) support (Issue #2067)


@testset "titlecase(::Char)" begin
    @test titlecase('a') == 'A'
    @test titlecase('z') == 'Z'
    @test titlecase('A') == 'A'
    @test titlecase('1') == '1'
end

@testset "titlecase still works on String" begin
    @test titlecase("hello world") == "Hello World"
    @test titlecase("HELLO") == "Hello"
end

# ===== source: strings/uppercase_lowercase_char.jl =====
# uppercase(c::Char) and lowercase(c::Char) support (Issue #2064)


@testset "uppercase(::Char)" begin
    @test uppercase('a') == 'A'
    @test uppercase('z') == 'Z'
    @test uppercase('A') == 'A'
    @test uppercase('1') == '1'
end

@testset "lowercase(::Char)" begin
    @test lowercase('A') == 'a'
    @test lowercase('Z') == 'z'
    @test lowercase('a') == 'a'
    @test lowercase('1') == '1'
end

@testset "uppercase/lowercase still work on String" begin
    @test uppercase("hello") == "HELLO"
    @test lowercase("HELLO") == "hello"
end

# ===== source: strings/vector_char_equality.jl =====
# Vector{Char} equality comparison (Issue #2032)
# Regression test: comparing two Vector{Char} arrays with == should work.


@testset "Vector{Char} equality (Issue #2032)" begin
    # collect(string) == char array literal
    @test collect("abc") == ['a', 'b', 'c']
    @test collect("hello") == ['h', 'e', 'l', 'l', 'o']

    # Inequality: different content
    @test (collect("abc") == ['a', 'b', 'd']) == false

    # Inequality: different length
    @test (collect("abc") == ['a', 'b']) == false

    # Char array literal == char array literal
    @test ['x', 'y'] == ['x', 'y']
    @test (['x', 'y'] == ['x', 'z']) == false

    # != operator
    @test collect("abc") != ['x', 'y', 'z']
end

true
