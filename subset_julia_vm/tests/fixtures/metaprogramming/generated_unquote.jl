# Phase 3 @generated unquoting tests
# Tests for compile-time unquoting of quote expressions in @generated blocks
#
# Note: SubsetJuliaVM Phase 3 unquotes simple quote patterns at compile time.
# For patterns compatible with official Julia's @generated semantics, we test
# that both implementations produce the same results.

using Test

# Test 1: Single quote expression (existing functionality)
# The generated branch returns :(x^2), which is compiled as the function body
function single_quote_gen(x)
    if @generated
        :(x^2)
    else
        x^2
    end
end

# Test 2: Return with quote expression (new)
# The return statement with a quote works the same in both implementations
function return_quote_gen(x, y)
    if @generated
        return :(x + y)
    else
        return x + y
    end
end

# Test 3: Quote with function call (new)
# Function calls inside quotes work correctly
function funcall_quote_gen(x)
    if @generated
        :(sin(x))
    else
        sin(x)
    end
end

# Test 4: Quote with binary operators (new)
# Complex expressions with multiple operators
function binop_quote_gen(x, y)
    if @generated
        :(x * y + 1)
    else
        x * y + 1
    end
end

# Test 5: Quote with nested function calls (new)
function nested_call_gen(x)
    if @generated
        :(abs(sin(x)))
    else
        abs(sin(x))
    end
end

@testset "Phase 3 @generated unquoting" begin
    @test single_quote_gen(3) == 9
    @test single_quote_gen(4) == 16

    @test return_quote_gen(3, 4) == 7
    @test return_quote_gen(10, 20) == 30

    @test funcall_quote_gen(0.0) == 0.0
    @test abs(funcall_quote_gen(1.5707963267948966) - 1.0) < 1e-10  # sin(pi/2) ≈ 1.0

    @test binop_quote_gen(3, 4) == 13   # 3 * 4 + 1 = 13
    @test binop_quote_gen(5, 2) == 11   # 5 * 2 + 1 = 11

    @test nested_call_gen(0.0) == 0.0
    @test abs(nested_call_gen(-1.5707963267948966) - 1.0) < 1e-10  # abs(sin(-pi/2)) = 1.0
end

true
