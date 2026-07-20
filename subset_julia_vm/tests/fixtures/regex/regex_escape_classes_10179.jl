using Test

# Issues #10179 / #10180 / #10203: PCRE2 escape-class parity at Regex
# construction. Upstream Julia (PCRE2) accepts octal / \o{} / \cX escapes,
# and treats \v / \h / \H / \V as whitespace classes; fancy-regex does not.
# A pattern-preprocessing pass in RegexValue::new rewrites these before
# compilation. Every @test below was verified against upstream julia 1.12.

@testset "octal / \\o{} / \\cX escapes (Issue #10179)" begin
    # \ddd octal (no capture groups) -> the character with that octal code.
    @test occursin(r"\101", "A") == true          # octal 101 = 0x41 = 'A'
    @test occursin(r"\101", "B") == false
    @test match(r"\50", "(").match == "("         # octal 50 = 0x28 = '('
    @test match(r"\11", "\t").match == "\t"        # octal 11 = 0x09 = TAB
    @test match(r"\0", "\0").match == "\0"         # \0 = NUL
    # Up to three octal digits, then literal characters.
    @test match(r"\1010", "A0").match == "A0"
    # \o{...} braced octal.
    @test occursin(r"\o{101}", "A") == true
    @test match(r"\o{12}", "\n").match == "\n"
    # \cX control escape (case-folded, XOR 0x40).
    @test occursin(r"\cA", "\x01") == true
    @test occursin(r"\cZ", "\x1a") == true
    @test occursin(r"\ca", "\x01") == true
    # Octal is also valid inside a character class.
    @test occursin(r"[\101]", "A") == true
end

@testset "genuine back references still work (Issue #10179)" begin
    @test occursin(r"(a)\1", "aa") == true
    @test occursin(r"(a)\1", "ab") == false
    @test match(r"(ab)\1", "abab").match == "abab"
end

@testset "vertical-whitespace class \\v / \\V (Issue #10180)" begin
    @test occursin(r"\v", "a\nb") == true          # \n is vertical whitespace
    @test occursin(r"\v", "ab") == false
    @test occursin(r"\v", "a\rb") == true          # CR
    @test occursin(r"\v", "a\fb") == true          # FF
    @test match(r"\v+", "a\n\r\fb").match == "\n\r\f"
    # Complement.
    @test occursin(r"\V", "\n") == false
    @test occursin(r"\V", "a") == true
    # Inside a character class.
    @test occursin(r"[\v]", "\n") == true
    @test occursin(r"[\va]", "a") == true
end

@testset "horizontal-whitespace class \\h / \\H (Issue #10203)" begin
    @test occursin(r"\h", "ab") == false           # no horizontal whitespace
    @test occursin(r"\h", "a b") == true           # space
    @test occursin(r"\h", "a\tb") == true          # tab
    @test match(r"\h+", "a \t b").match == " \t "
    # NBSP and Unicode spaces are horizontal whitespace.
    @test occursin(r"\h", "a b") == true
    @test occursin(r"\h", "a　b") == true
    @test occursin(r"\h", "a\nb") == false         # newline is NOT horizontal
    # Complement \H.
    @test occursin(r"\H", "   ") == false
    @test occursin(r"\H", "  a") == true
    # Inside a character class (positive body and nested-negated complement).
    @test occursin(r"[\h]", "\t") == true
    @test occursin(r"[\h]", "x") == false
    @test occursin(r"[a\H]", "b") == true
    @test occursin(r"[a\H]", " ") == false
end

true
