# Test global variable compound assignment with multiplication
# Issue #357: Parser does not support 'global var += expr' syntax

using Test

@testset "Global variable compound multiplication (global x *= expr)" begin

    # Test global *= in a for loop
    total = 1.0
    for i in 1:4
        global total *= i
    end

    # total should be 1 * 1 * 2 * 3 * 4 = 24
    @test (total) == 24.0
end

true  # Test passed
