using Test

# `round(x)` with the default `RoundNearest` mode uses round-half-to-even
# (banker's rounding) in Julia: a tie is rounded to the nearest EVEN integer, not
# away from zero. sjulia used Rust's `f64::round` (half away from zero), so
# `round(2.5)` wrongly gave `3.0` instead of `2.0`. (Verified with
# `scripts/fixture_julia_parity.sh`, which compares the pass/fail counts under
# sjulia and upstream julia — the nextest harness only checks the fixture's final
# returned value, so a banker's-rounding regression must be caught by parity.)

@testset "round(x) ties to even (RoundNearest), Issue round-bankers" begin
    @test round(0.5) == 0.0
    @test round(1.5) == 2.0
    @test round(2.5) == 2.0
    @test round(3.5) == 4.0
    @test round(4.5) == 4.0
    @test round(-0.5) == 0.0
    @test round(-1.5) == -2.0
    @test round(-2.5) == -2.0
    # Non-tie values round to the nearest as usual.
    @test round(0.4) == 0.0
    @test round(0.6) == 1.0
    @test round(2.4) == 2.0
    @test round(2.6) == 3.0
    # Float32 ties also round to even.
    @test round(2.5f0) == 2.0f0
    @test round(0.5f0) == 0.0f0
end

@testset "round(x, digits=n) / sigdigits=n tie to even" begin
    @test round(2.5, digits=0) == 2.0
    @test round(1.25, digits=1) == 1.2
    @test round(0.125, digits=2) == 0.12
    @test round(2.5, sigdigits=1) == 2.0
end

true
