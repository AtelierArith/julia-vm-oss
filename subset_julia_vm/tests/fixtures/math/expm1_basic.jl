# Test expm1() function (Issue #2095)

using Test

@testset "expm1 - exp(x) - 1" begin
    # Basic values
    @test expm1(0.0) == 0.0
    @test isapprox(expm1(1.0), exp(1.0) - 1.0)
    @test isapprox(expm1(-1.0), exp(-1.0) - 1.0)

    # Small values (where accuracy matters)
    @test isapprox(expm1(0.001), exp(0.001) - 1.0)
    @test isapprox(expm1(0.01), exp(0.01) - 1.0)
    @test isapprox(expm1(-0.001), exp(-0.001) - 1.0)

    # Larger values
    @test isapprox(expm1(2.0), exp(2.0) - 1.0)
    @test isapprox(expm1(5.0), exp(5.0) - 1.0)

    # Integer input
    @test isapprox(expm1(1), exp(1) - 1.0)
    @test expm1(0) == 0.0

    # Negative values
    @test isapprox(expm1(-2.0), exp(-2.0) - 1.0)
end

true
