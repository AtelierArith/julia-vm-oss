using Test

# Tests for IEEE 754 float arithmetic edge cases.
# Per the IEEE 754 standard (and Julia semantics), floating-point operations
# involving infinity or NaN do NOT throw exceptions — they return special values.
# SubsetJuliaVM must match this behavior.

@testset "float division by zero produces Inf (IEEE 754)" begin
    @test 1.0 / 0.0 == Inf
    @test -1.0 / 0.0 == -Inf
    @test isinf(1.0 / 0.0)
    @test isinf(-1.0 / 0.0)
    @test 1.0 / 0.0 > 0.0
    @test -1.0 / 0.0 < 0.0
end

@testset "zero divided by zero produces NaN (IEEE 754)" begin
    @test isnan(0.0 / 0.0)
    @test !isinf(0.0 / 0.0)
    # NaN is not equal to anything, including itself
    @test 0.0 / 0.0 != 0.0 / 0.0
end

@testset "Inf arithmetic follows IEEE 754" begin
    @test isnan(Inf - Inf)
    @test isnan(Inf + (-Inf))
    @test Inf + Inf == Inf
    @test -Inf - Inf == -Inf
    @test Inf * 2.0 == Inf
    @test -Inf * 2.0 == -Inf
    @test isnan(0.0 * Inf)
    @test 1.0 / Inf == 0.0
    @test Inf / Inf |> isnan
end

@testset "NaN propagates through arithmetic" begin
    nan = 0.0 / 0.0
    @test isnan(nan + 1.0)
    @test isnan(nan - 1.0)
    @test isnan(nan * 2.0)
    @test isnan(nan / 2.0)
    @test isnan(1.0 + nan)
end

@testset "isinf and isnan predicates" begin
    @test isinf(Inf)
    @test isinf(-Inf)
    @test !isinf(1.0)
    @test !isinf(0.0)
    @test isnan(NaN)
    @test !isnan(1.0)
    @test !isnan(Inf)
    # isfinite
    @test isfinite(1.0)
    @test isfinite(0.0)
    @test !isfinite(Inf)
    @test !isfinite(-Inf)
    @test !isfinite(NaN)
end

true
