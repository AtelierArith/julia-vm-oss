# Test global variable compound assignment syntax
# Issue #357: Parser does not support 'global var += expr' syntax

using Test

@testset "Global variable compound assignment syntax (global x += expr)" begin

    # Test global += in a for loop
    total = 0.0
    for i in 1:5
        global total += i
    end

    # total should be 1 + 2 + 3 + 4 + 5 = 15
    @test (total) == 15.0
end

true  # Test passed
