# Test arithmetic operations with missing
# All arithmetic with missing returns missing
#
# Note: min/max with missing require runtime dispatch which is tracked in Issue #719.
# This test focuses on literal missing values which are handled at compile-time.

using Test

@testset "Missing arithmetic" begin
    # Test that missing + missing returns missing
    r1 = missing + missing
    @test ismissing(r1)

    # Test missing + number
    r2 = missing + 1
    @test ismissing(r2)

    # Test number + missing
    r3 = 1 + missing
    @test ismissing(r3)

    # Test missing - missing
    r4 = missing - missing
    @test ismissing(r4)

    # Test missing * missing
    r5 = missing * missing
    @test ismissing(r5)

    # Test missing / missing
    r6 = missing / missing
    @test ismissing(r6)

    # Test unary minus
    r7 = -missing
    @test ismissing(r7)
end

true
