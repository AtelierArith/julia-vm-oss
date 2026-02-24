# Test codepoint() function - get Unicode code point of character
# codepoint(c::Char) -> UInt32

using Test

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

true  # Test passed
