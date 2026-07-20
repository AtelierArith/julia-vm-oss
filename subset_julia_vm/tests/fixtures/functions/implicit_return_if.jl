# Test: Implicit return from if/else blocks (Issue #1119)
# In Julia, if/else blocks can serve as the return value of a function
# without explicit return statements.

using Test

# Function with if/else as implicit return
function safe_increment(x)
    if x === nothing
        0  # Implicit return
    else
        x + 1  # Implicit return
    end
end

# Function with nested if/else
function categorize(x)
    if x < 0
        "negative"
    elseif x == 0
        "zero"
    else
        "positive"
    end
end

@testset "Implicit return from if/else" begin
    # Test safe_increment
    @test safe_increment(nothing) == 0
    @test safe_increment(5) == 6
    @test safe_increment(0) == 1

    # Test categorize (nested if/elseif/else)
    @test categorize(-5) == "negative"
    @test categorize(0) == "zero"
    @test categorize(10) == "positive"
end

true
