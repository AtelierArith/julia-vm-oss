using Test

@testset "Bool strong zero multiplication" begin
    # Julia strong zero: false * x == copysign(zero(x), x)
    # This means false * NaN == 0.0 (not NaN), false * Inf == 0.0 (not NaN)

    # false * NaN == 0.0 (strong zero)
    @test false * NaN === 0.0
    @test NaN * false === 0.0

    # false * Inf == 0.0 (strong zero)
    @test false * Inf === 0.0
    @test Inf * false === 0.0

    # false * -Inf == -0.0 (preserves sign via copysign)
    @test false * -Inf === -0.0
    @test -Inf * false === -0.0

    # true preserves value
    @test true * NaN === NaN
    @test NaN * true === NaN
    @test true * Inf === Inf
    @test Inf * true === Inf

    # Normal multiplication
    @test false * 3.14 === 0.0
    @test true * 3.14 === 3.14
    @test 3.14 * false === 0.0
    @test 3.14 * true === 3.14
end

true
