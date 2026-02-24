# Test Char comparison operators
# Issue #945: Char comparison (==) not implemented

using Test

@testset "Char comparison operators" begin
    # Basic equality
    @test ('a' == 'a') == true
    @test ('a' == 'b') == false

    # Inequality
    @test ('a' != 'b') == true
    @test ('a' != 'a') == false

    # With variables
    c1 = 'a'
    c2 = 'a'
    c3 = 'b'
    @test (c1 == c2) == true
    @test (c1 == c3) == false
    @test (c1 != c3) == true

    # In array context
    chars = Char[]
    push!(chars, 'a')
    @test (chars[1] == 'a') == true
    @test (chars[1] != 'b') == true
end

true
