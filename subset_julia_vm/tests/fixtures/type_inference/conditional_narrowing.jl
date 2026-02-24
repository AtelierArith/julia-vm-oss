# Test: Conditional type narrowing
# Tests that type information flows through conditional branches
using Test

# Function must be defined OUTSIDE @testset block per project guidelines
function numeric_check(x)
    # Test conditional narrowing with numeric comparison
    if x > 0
        x * 2  # x is known to be positive numeric here
    else
        0
    end
end

function string_length_check(s)
    # Test conditional with string length
    if length(s) > 0
        length(s)
    else
        -1
    end
end

function bool_check(flag)
    # Test boolean conditional
    if flag
        1
    else
        0
    end
end

function comparison_narrowing(x, y)
    # Test narrowing from comparison
    if x > y
        x - y
    else
        y - x
    end
end

@testset "Conditional type narrowing" begin
    # Numeric conditional tests
    @test numeric_check(5) == 10
    @test numeric_check(-3) == 0
    @test numeric_check(0) == 0
    
    # String length conditional tests
    @test string_length_check("hello") == 5
    @test string_length_check("") == -1
    
    # Boolean conditional tests
    @test bool_check(true) == 1
    @test bool_check(false) == 0
    
    # Comparison narrowing tests
    @test comparison_narrowing(10, 3) == 7
    @test comparison_narrowing(3, 10) == 7
end

true
