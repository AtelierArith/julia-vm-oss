using Test

# Issue #6796: BigFloat `%` / `rem` / `mod` were unsupported
# ("Unsupported BigFloat operation: Mod"). Now computed via a RemBigFloat
# intrinsic (astro_float `rem`): `%`/`rem` take the sign of the dividend,
# `mod` the sign of the divisor (built on `%` in base/math.jl), and `x % 0`
# is NaN (like Float64), all matching upstream Julia.

@testset "BigFloat % operator (Issue #6796)" begin
    @test big(5.0) % big(3.0) == 2
    @test big(-5.0) % big(3.0) == -2
    @test big(5.0) % big(-3.0) == 2
    @test big(5.5) % big(2.0) == 1.5
    @test typeof(big(5.0) % big(3.0)) === BigFloat
end

@testset "BigFloat rem (sign of dividend) (Issue #6796)" begin
    @test rem(big(5.0), big(3.0)) == 2
    @test rem(big(-5.0), big(3.0)) == -2
    @test typeof(rem(big(5.0), big(3.0))) === BigFloat
end

@testset "BigFloat mod (sign of divisor) (Issue #6796)" begin
    @test mod(big(5.0), big(3.0)) == 2
    @test mod(big(-5.0), big(3.0)) == 1
    @test mod(big(5.0), big(-3.0)) == -1
    @test typeof(mod(big(5.0), big(3.0))) === BigFloat
end

@testset "BigFloat % mixed operands and zero divisor (Issue #6796)" begin
    @test big(5.0) % 3 == 2
    @test 5.0 % big(3.0) == 2
    @test isnan(big(5.0) % big(0.0))
    @test typeof(big(5.0) % 3) === BigFloat
end

@testset "BigFloat % through user functions (Issue #6796)" begin
    f(x, y) = x % y
    g(x, y) = rem(x, y)
    h(x, y) = mod(x, y)
    @test f(big(5.0), big(3.0)) == 2
    @test g(big(-5.0), big(3.0)) == -2
    @test h(big(-5.0), big(3.0)) == 1
end

true
