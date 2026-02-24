# Test @show macro with user-defined functions (Issue #1330)
# The @show macro should correctly display and return the value
# when called with user-defined function calls.

using Test

# Define a simple user function using short syntax
f(x) = 2x + 1

# Define a regular function
function triple(n)
    3 * n
end

@testset "@show with user-defined functions" begin
    # Test 1: @show with short function definition
    # The @show macro should print "f(3) = 7" and return 7
    result1 = @show f(3)
    @test result1 == 7

    # Test 2: @show with regular function definition
    result2 = @show triple(4)
    @test result2 == 12

    # Test 3: @show with expression involving user function
    result3 = @show f(2) + triple(1)
    @test result3 == 8  # f(2)=5, triple(1)=3, 5+3=8
end

true
