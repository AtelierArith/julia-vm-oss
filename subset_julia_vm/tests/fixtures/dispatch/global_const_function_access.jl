# Test that functions can access global const values from prelude
# Issue #1443: Global const values not accessible from function bodies

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
