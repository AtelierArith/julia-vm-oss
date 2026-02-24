# Char arithmetic and Char↔Int conversion (Issue #2035)
# Char values can be converted to/from integers and used in arithmetic expressions.

using Test

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

true
