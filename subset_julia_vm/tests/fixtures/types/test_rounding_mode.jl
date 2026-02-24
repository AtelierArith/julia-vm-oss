# Test RoundingMode type and constants (Issue #428)
# Note: SubsetJuliaVM uses struct with Symbol field, Julia uses parametric type

using Test

@testset "RoundingMode type and constants" begin
    # Test RoundingMode struct exists and constants are instances
    @test isa(RoundNearest, RoundingMode)
    @test isa(RoundToZero, RoundingMode)
    @test isa(RoundUp, RoundingMode)
    @test isa(RoundDown, RoundingMode)
    @test isa(RoundFromZero, RoundingMode)
    @test isa(RoundNearestTiesAway, RoundingMode)
    @test isa(RoundNearestTiesUp, RoundingMode)

    # Test that different rounding modes are not equal (identity test)
    @test RoundNearest !== RoundToZero
    @test RoundUp !== RoundDown
    @test RoundFromZero !== RoundToZero

    # Test total count of 7 standard rounding modes
    modes = [RoundNearest, RoundToZero, RoundUp, RoundDown,
             RoundFromZero, RoundNearestTiesAway, RoundNearestTiesUp]
    @test length(modes) == 7
end

true  # Test passed
