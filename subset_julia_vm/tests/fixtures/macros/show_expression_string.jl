# Test @show macro displays source expression, not evaluated value
# This test verifies the fix for issue #1352

using Test

# Define a simple function for testing
f(x) = x + 1
g(a, b) = a * b

@testset "@show displays source expression" begin
    # Basic function call
    result = @show f(5)
    @test result == 6
    
    # Function with multiple arguments
    result2 = @show g(3, 4)
    @test result2 == 12
    
    # Simple variable
    x = 42
    result3 = @show x
    @test result3 == 42
    
    # Arithmetic expression
    result4 = @show 2 + 3
    @test result4 == 5
    
    # Nested function call
    result5 = @show f(f(1))
    @test result5 == 3
end

true
