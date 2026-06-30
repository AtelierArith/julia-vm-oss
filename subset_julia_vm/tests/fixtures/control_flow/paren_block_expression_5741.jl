using Test
@testset "parenthesized multi-statement block (Issue #5741)" begin
    @test (x = 1; x + 1) == 2
    @test (a = 2; b = 3; a + b) == 5
    @test (x = 5; y = 10; x * y) == 50
    # block returns the value of its last statement
    r = (p = 10; q = 20; p + q)
    @test r == 30
    # single trailing-semicolon
    @test (5;) == 5
    # block with a function call as the last statement
    @test (v = [1, 2, 3]; sum(v)) == 6

    # Controls: arrow functions with semicolon keyword params still work
    f = (x; y = 1) -> x + y
    @test f(5) == 6
    @test f(5; y = 100) == 105
    # Controls: named tuples still work
    @test (; a = 1) == (a = 1,)
    @test (; a = 1, b = 2) == (a = 1, b = 2)
    @test (;) == NamedTuple()
    # Control: plain tuple
    @test (1, 2) == (1, 2)
end
true
