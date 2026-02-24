# Test: Union type inference in conditionals
using Test

# Function must be defined OUTSIDE @testset block per project guidelines
function mixed_return(flag)
    if flag
        1        # Int64
    else
        2.0      # Float64
    end
    # Return type: Union{Int64, Float64}
end

function conditional_types(x)
    # Returns different types based on input
    if x > 0
        x * 2        # Int64 when x is Int
    else
        0.0          # Float64
    end
end

function nested_conditionals(a, b)
    if a
        if b
            1
        else
            2
        end
    else
        3
    end
end

function multiple_branches(n)
    # Multiple branches returning different types
    if n < 0
        -1
    elseif n == 0
        0
    else
        1
    end
end

@testset "Union type inference" begin
    # Test mixed return types
    result1 = mixed_return(true)
    result2 = mixed_return(false)
    
    @test result1 == 1
    @test result2 == 2.0
    
    # Test conditional type inference
    @test conditional_types(5) == 10
    @test conditional_types(-1) == 0.0
    
    # Test nested conditionals
    @test nested_conditionals(true, true) == 1
    @test nested_conditionals(true, false) == 2
    @test nested_conditionals(false, true) == 3
    
    # Test multiple branches
    @test multiple_branches(-5) == -1
    @test multiple_branches(0) == 0
    @test multiple_branches(5) == 1
end

true
