# Test global variable compound assignment with subtraction
# Issue #357: Parser does not support 'global var += expr' syntax

using Test

# Keep the target as a real module global. A same-named @testset local followed
# by `global total -= ...` is rejected by upstream Julia (Issue #11599).
global total_compound_sub_357 = 100.0

@testset "Global variable compound subtraction (global x -= expr)" begin
    # Test global -= in a for loop
    for i in 1:5
        global total_compound_sub_357 -= i
    end

    # total should be 100 - 1 - 2 - 3 - 4 - 5 = 85
    @test total_compound_sub_357 == 85.0
end

true  # Test passed
