# Test more exported string functions

using Test

@testset "More string function exports" begin
    # isnumeric - check if character is numeric (Unicode)
    @test isnumeric('5')
    @test !isnumeric('a')

    # isvalid - check if index is valid character boundary
    s = "hello"
    @test isvalid(s, 1)
    @test isvalid(s, 5)

    # reverseind - convert index for reversed string
    @test reverseind(s, 1) == 5
    @test reverseind(s, 5) == 1

    # unescape_string - unescape escape sequences
    @test unescape_string("a\\nb") === "a\nb"
    @test unescape_string("a\\tb") === "a\tb"
end

true
