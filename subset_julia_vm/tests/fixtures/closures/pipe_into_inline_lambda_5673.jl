using Test
@testset "piping into an inline lambda (Issue #5673)" begin
    @test (1:3 |> x -> collect(x)) == [1, 2, 3]
    @test (5 |> x -> x + 1) == 6
    @test ([1, 2, 3] |> x -> sum(x)) == 6
    @test ("abc" |> s -> uppercase(s)) == "ABC"
    @test (10 |> x -> x * 2) == 20
    # named-function pipe still works (control)
    f = x -> x + 1
    @test (5 |> f) == 6
    # parenthesized lambda still works (control)
    @test (5 |> (x -> x + 1)) == 6
    # arrow in other positions unaffected (controls)
    @test map(x -> x^2, [1, 2, 3]) == [1, 4, 9]
    @test filter(x -> x > 2, [1, 2, 3, 4]) == [3, 4]
    g = (x, y) -> x + y
    @test g(3, 4) == 7
    @test (x -> x > 0 ? 1 : -1)(5) == 1
end
true
