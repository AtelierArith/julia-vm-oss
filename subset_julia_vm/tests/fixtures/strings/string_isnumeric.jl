# Test isnumeric - Unicode numeric character check (Issue #6752)
#
# isnumeric is now Pure Julia (base/strings/unicode.jl): it binary-searches an
# embedded Nd/Nl/No codepoint range table generated from upstream julia's
# utf8proc, replacing the Rust `BuiltinId::Isnumeric` (`char::is_numeric()`).
# The previous fixture was ASCII-only and could not catch a regression in the
# non-ASCII Nd/Nl/No coverage; these cases are verified to match upstream julia.

using Test

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

true
