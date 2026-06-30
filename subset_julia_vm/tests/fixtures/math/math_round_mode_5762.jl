using Test

# Issue #5762: round(x, mode::RoundingMode) must honor the mode. Previously the
# mode was ignored (RoundUp/RoundDown/RoundToZero behaved like the default).

@testset "round(x, RoundingMode) (Issue #5762)" begin
    # RoundUp == ceil
    @test round(2.5, RoundUp) == 3.0
    @test round(2.3, RoundUp) == 3.0
    @test round(-2.5, RoundUp) == -2.0

    # RoundDown == floor
    @test round(2.7, RoundDown) == 2.0
    @test round(2.1, RoundDown) == 2.0
    @test round(-2.1, RoundDown) == -3.0

    # RoundToZero == trunc
    @test round(2.9, RoundToZero) == 2.0
    @test round(-2.9, RoundToZero) == -2.0
    @test round(-2.5, RoundToZero) == -2.0

    # RoundNearest is round-half-to-even (banker's)
    @test round(2.5, RoundNearest) == 2.0
    @test round(3.5, RoundNearest) == 4.0
    @test round(2.4, RoundNearest) == 2.0

    # The default 1-arg round is unchanged
    @test round(2.5) == 2.0
    @test round(2.7) == 3.0
end

true
