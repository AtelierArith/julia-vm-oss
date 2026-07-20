# Test character classification functions: iscntrl, ispunct, isxdigit (Issue #1874)

using Test

@testset "iscntrl" begin
    @test iscntrl('\n') == true
    @test iscntrl('\t') == true
    @test iscntrl('\r') == true
    @test iscntrl('a') == false
    @test iscntrl(' ') == false
    @test iscntrl('0') == false
end

@testset "ispunct" begin
    @test ispunct('!') == true
    @test ispunct('.') == true
    @test ispunct(',') == true
    @test ispunct(':') == true
    @test ispunct('?') == true
    @test ispunct('@') == true
    @test ispunct('[') == true
    @test ispunct('{') == true
    # ASCII symbol characters (Unicode Sc/Sm/Sk) are NOT punctuation
    # upstream (Issues #10321 / #10237)
    @test ispunct('~') == false
    @test ispunct('$') == false
    @test ispunct('+') == false
    @test ispunct('<') == false
    @test ispunct('=') == false
    @test ispunct('>') == false
    @test ispunct('^') == false
    @test ispunct('`') == false
    @test ispunct('|') == false
    @test ispunct('a') == false
    @test ispunct('0') == false
    @test ispunct(' ') == false
    # Non-ASCII Unicode punctuation (Pc/Pd/Ps/Pe/Pi/Pf/Po) IS punctuation
    # upstream (Issue #10321)
    @test ispunct('¡') == true   # U+00A1 INVERTED EXCLAMATION MARK (Po)
    @test ispunct('¿') == true   # U+00BF INVERTED QUESTION MARK (Po)
    @test ispunct('«') == true   # U+00AB LEFT-POINTING GUILLEMET (Pi)
    @test ispunct('»') == true   # U+00BB RIGHT-POINTING GUILLEMET (Pf)
    @test ispunct('§') == true   # U+00A7 SECTION SIGN (Po)
    @test ispunct('¶') == true   # U+00B6 PILCROW SIGN (Po)
    @test ispunct('–') == true   # U+2013 EN DASH (Pd)
    @test ispunct('—') == true   # U+2014 EM DASH (Pd)
    @test ispunct('…') == true   # U+2026 HORIZONTAL ELLIPSIS (Po)
    @test ispunct('。') == true  # U+3002 IDEOGRAPHIC FULL STOP (Po)
    # Non-ASCII non-punctuation stays false
    @test ispunct('α') == false  # U+03B1 GREEK SMALL LETTER ALPHA (Ll)
    @test ispunct('°') == false  # U+00B0 DEGREE SIGN (So)
    @test ispunct('¬') == false  # U+00AC NOT SIGN (Sm)
end

@testset "isxdigit" begin
    @test isxdigit('0') == true
    @test isxdigit('9') == true
    @test isxdigit('a') == true
    @test isxdigit('f') == true
    @test isxdigit('A') == true
    @test isxdigit('F') == true
    @test isxdigit('g') == false
    @test isxdigit('G') == false
    @test isxdigit('z') == false
    @test isxdigit(' ') == false
end

true
