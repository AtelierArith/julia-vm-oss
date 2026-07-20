# Test global variable compound assignment syntax
# Issue #357: Parser does not support 'global var += expr' syntax

using Test

# The compound-assignment target must be a real module global before entering
# the @testset hard scope. Declaring `global total` after a same-named testset
# local is a syntax error in upstream Julia (Issue #11599).
global total_compound_357 = 0.0

@testset "Global variable compound assignment syntax (global x += expr)" begin
    # Test global += in a for loop
    for i in 1:5
        global total_compound_357 += i
    end

    # total should be 1 + 2 + 3 + 4 + 5 = 15
    @test total_compound_357 == 15.0
end

true  # Test passed
