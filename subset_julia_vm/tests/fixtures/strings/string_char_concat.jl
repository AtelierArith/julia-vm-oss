# Test String * Char concatenation (Issue #2127)

using Test

@testset "String * Char concatenation" begin
    # Basic String * Char
    @test "Hello" * '!' == "Hello!"
    @test '>' * "arrow" == ">arrow"

    # Char * Char
    @test 'a' * 'b' == "ab"

    # typeof checks
    @test typeof("abc" * 'd') == String
    @test typeof('a' * "bc") == String
    @test typeof('a' * 'b') == String

    # Chained concatenation (n-ary reduction path)
    s = "Hello" * ' ' * "World"
    @test s == "Hello World"

    result = "a" * 'b' * "c" * 'd'
    @test result == "abcd"

    # Multi-step (to verify consistency)
    a = "Hello" * ' '
    r = a * "World"
    @test r == "Hello World"
end

true
