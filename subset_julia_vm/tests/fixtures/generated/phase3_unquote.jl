# Test: Phase 3 - Simple quoted expression unquoting
#
# NOTE: This test is SubsetJuliaVM-specific and will NOT pass in standard Julia.
# In Julia, `if @generated` executes the generated branch which returns an Expr object.
# In SubsetJuliaVM Phase 3, we "unquote" the expression and execute it directly.
#
# This test verifies that simple quoted expressions in the @generated branch
# are "unquoted" and executed directly.
#
# Pattern:
#   if @generated
#       result = :(expr)  # Simple quoted expression - Phase 3 unquotes this
#   else
#       result = expr     # Fallback
#   end
#
# In SubsetJuliaVM:
#   - Phase 3 detects `result = :(x^2)` and transforms to `result = x^2`
#   - condition=true, so the transformed generated branch executes

using Test

function square_gen(x)
    result = 0
    if @generated
        result = :(x^2)
    else
        result = x^2
    end
    result
end

function double_gen(x)
    result = 0
    if @generated
        result = :(x * 2)
    else
        result = x * 2
    end
    result
end

function add_one_gen(x)
    result = 0
    if @generated
        result = :(x + 1)
    else
        result = x + 1
    end
    result
end

function complex_gen(x, y)
    result = 0
    if @generated
        result = :(x * y + 1)
    else
        result = x * y + 1
    end
    result
end

@testset "Phase 3: Simple quoted expressions are unquoted and executed (SubsetJuliaVM-only, will fail in standard Julia)" begin

    # Test 1: Simple binary expression - power

    r1 = square_gen(5)
    println("square_gen(5) = ", r1)
    @assert r1 == 25

    r2 = square_gen(3)
    println("square_gen(3) = ", r2)
    @assert r2 == 9

    # Test 2: Binary expression - multiplication

    r3 = double_gen(7)
    println("double_gen(7) = ", r3)
    @assert r3 == 14

    # Test 3: Binary expression - addition

    r4 = add_one_gen(10)
    println("add_one_gen(10) = ", r4)
    @assert r4 == 11

    # Test 4: Multiple operations

    r5 = complex_gen(3, 4)
    println("complex_gen(3, 4) = ", r5)
    @assert r5 == 13

    # Return true if all tests passed
    @test (true)
end

true  # Test passed
