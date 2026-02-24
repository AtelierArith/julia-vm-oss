# Test textwidth() function - get display width of string
# textwidth(s::String) -> Int64
# textwidth(c::Char) -> Int64

using Test

@testset "textwidth() - get display width of string/character" begin

    result = 0

    # Test ASCII string (each character has width 1)
    if textwidth("hello") == 5
        result = result + 1
    end

    # Test empty string
    if textwidth("") == 0
        result = result + 1
    end

    # Test single character
    if textwidth("A") == 1
        result = result + 1
    end

    # Test character function
    if textwidth('A') == 1
        result = result + 1
    end

    @test (result) == 4
end

true  # Test passed
