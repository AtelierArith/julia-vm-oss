# Test @showtime macro displays source expression with timing
# Similar to @show but includes execution time

using Test

# Define a simple function for testing
f(x) = x + 1

@testset "@showtime displays source expression" begin
    # Basic function call
    result = @showtime f(5)
    @test result == 6
    
    # Arithmetic expression
    result2 = @showtime 2 + 3
    @test result2 == 5
end

true
