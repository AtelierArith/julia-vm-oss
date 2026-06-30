# Test string-literal hex/unicode/octal escape sequences (Issue #3569)

using Test

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

true  # Test passed
