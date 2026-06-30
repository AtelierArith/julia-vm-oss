using Test

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

true
