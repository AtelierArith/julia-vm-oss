using Test

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

true
