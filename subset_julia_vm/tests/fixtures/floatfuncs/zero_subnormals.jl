# Test get_zero_subnormals() and set_zero_subnormals() (Issue #441)
# These functions control whether subnormal/denormal floats are flushed to zero

using Test

@testset "Zero subnormals control" begin
    # Test get_zero_subnormals - should return false by default
    # (IEEE standard: subnormals are preserved)
    @test get_zero_subnormals() == false

    # Test set_zero_subnormals(false) - should succeed (return true)
    # since we're keeping the default IEEE-compliant behavior
    @test set_zero_subnormals(false) == true

    # Test set_zero_subnormals(true) - should fail (return false)
    # since SubsetJuliaVM doesn't support flush-to-zero mode
    @test set_zero_subnormals(true) == false

    # Verify state is still false after failed set
    @test get_zero_subnormals() == false
end

true
