# `^`/`.^` bind tighter than a prefix unary operator: `-x^2 == -(x^2)`,
# not `(-x)^2`. Matches julia/src/julia-parser.scm `parse-unary`
# ("-2^3 is parsed as -(2^3)"). Issue #7232.

using Test

@testset "unary minus binds looser than power (Issue #7232)" begin
    x = 3.0
    a = 5
    b = 2

    # Scalar: unary minus over the whole power
    @test -x^2 == -9.0
    @test -2^2 == -4
    @test -3.0^2 == -9.0
    @test -a^b == -25
    # Right-associative power, then negate: -(2^(2^3)) == -(2^8) == -256
    @test -2^2^3 == -256
    # RHS keeps its own sign: -x^-2 == -(x^(-2))
    @test -x^-2 == -(x^(-2))
    # Binary minus is unaffected: 10 - x^2 == 10 - 9 == 1
    @test 10 - x^2 == 1.0

    # Broadcast: -v .^ 2 == -(v .^ 2)
    v = [1.0, 2.0, 3.0]
    @test -v .^ 2 == [-1.0, -4.0, -9.0]
    @test -(v) .^ 2 == [-1.0, -4.0, -9.0]
    @test (-v .^ 2 .+ 1) == [0.0, -3.0, -8.0]

    # The reported Gaussian: exp.(-(x .- t) .^ 2) must decay, not diverge
    xs = -1.0:0.5:1.0
    t = 0.0
    g = exp.(-(xs .- t) .^ 2)
    @test maximum(g) <= 1.0
    @test g == exp.(-((collect(xs) .- t) .^ 2))
end

true  # Test passed
