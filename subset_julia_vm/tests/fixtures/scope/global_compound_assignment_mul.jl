# Test global variable compound assignment with multiplication
# Issue #357: Parser does not support 'global var += expr' syntax

using Test

# Keep the target as a real module global. A same-named @testset local followed
# by `global total *= ...` is rejected by upstream Julia (Issue #11599).
global total_compound_mul_357 = 1.0

@testset "Global variable compound multiplication (global x *= expr)" begin
    # Test global *= in a for loop
    for i in 1:4
        global total_compound_mul_357 *= i
    end

    # total should be 1 * 1 * 2 * 3 * 4 = 24
    @test total_compound_mul_357 == 24.0
end

true  # Test passed
