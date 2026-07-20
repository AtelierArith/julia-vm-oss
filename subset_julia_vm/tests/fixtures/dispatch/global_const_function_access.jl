# Test that functions can access global const values from prelude
# Issue #1443: Global const values not accessible from function bodies
# NOTE: `RoundNearest.mode` is a SubsetJuliaVM-specific field accessor —
# upstream Julia's RoundingMode has no `.mode` field — so this fixture is
# marked skip_julia_test in manifest.toml (Issue #10237).

using Test

# Test 1: Access prelude const RoundNearest from user function
function get_rounding_mode()
    return RoundNearest.mode
end

# Test 2: Access prelude const RoundToZero from user function
function get_zero_mode()
    return RoundToZero.mode
end

@testset "Global const function access" begin
    # Test prelude const RoundingMode values are accessible from functions
    @test get_rounding_mode() == :Nearest
    @test get_zero_mode() == :ToZero
end

true
