using Test

# Issue #5763: a generator argument followed by `; kw=...` keyword arguments
# (`f(x for x in it; kw=v)`) failed to parse ("unexpected token ';', expected
# RParen") because the generator-in-call parser consumed the closing paren and
# never reached the keyword-argument separator. The generator is a single
# positional argument; the closing paren is now left for the shared call
# argument loop, which accepts the trailing `; kwargs`.

@testset "generator argument followed by keyword args (Issue #5763)" begin
    # The motivating case: sum/prod with an init keyword
    @test sum(x^2 for x in 1:3; init=10) == 24
    @test prod(x for x in 1:3; init=2) == 12

    # Filtered generator (`if` clause) followed by a keyword
    @test sum(x for x in 1:5 if x > 2; init=0) == 12

    # init keyword changes the result for an empty/seeded reduction
    @test maximum(x for x in [3, 1, 2]; init=0) == 3
    @test sum(x for x in 1:0; init=100) == 100

    # User function: generator positional arg + multiple keyword args
    f(gen; a=1, b=2) = sum(gen) + a + b
    @test f(x for x in 1:3; a=10, b=20) == 36
    @test f(x for x in 1:3) == 9   # no kwargs still works (defaults)

    # The bare parenthesized generator `(x for x in it)` is unaffected
    g = (x for x in 1:3)
    @test collect(g) == [1, 2, 3]

    # Generator with no keyword arg still works
    @test sum(x^2 for x in 1:3) == 14
    @test collect(x^2 for x in 1:3) == [1, 4, 9]
end

true
