# Vector{Char} equality comparison (Issue #2032)
# Regression test: comparing two Vector{Char} arrays with == should work.

using Test

@testset "Vector{Char} equality (Issue #2032)" begin
    # collect(string) == char array literal
    @test collect("abc") == ['a', 'b', 'c']
    @test collect("hello") == ['h', 'e', 'l', 'l', 'o']

    # Inequality: different content
    @test (collect("abc") == ['a', 'b', 'd']) == false

    # Inequality: different length
    @test (collect("abc") == ['a', 'b']) == false

    # Char array literal == char array literal
    @test ['x', 'y'] == ['x', 'y']
    @test (['x', 'y'] == ['x', 'z']) == false

    # != operator
    @test collect("abc") != ['x', 'y', 'z']
end

true
