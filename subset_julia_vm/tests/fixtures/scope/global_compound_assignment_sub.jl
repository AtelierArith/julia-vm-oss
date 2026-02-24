# Test global variable compound assignment with subtraction
# Issue #357: Parser does not support 'global var += expr' syntax

using Test

@testset "Global variable compound subtraction (global x -= expr)" begin

    # Test global -= in a for loop
    total = 100.0
    for i in 1:5
        global total -= i
    end

    # total should be 100 - 1 - 2 - 3 - 4 - 5 = 85
    @test (total) == 85.0
end

true  # Test passed
