# Test ::Array type annotation in function dispatch (Issue #662)
# This ensures that function parameters with ::Array type annotation work correctly

using LinearAlgebra

# Simple function with ::Array parameter
function sum_array(x::Array)
    s = 0.0
    for i in 1:length(x)
        s = s + x[i]
    end
    return s
end

# Function with two ::Array parameters
function dot_arrays(x::Array, y::Array)
    s = 0.0
    for i in 1:length(x)
        s = s + x[i] * y[i]
    end
    return s
end

# Function using array operations with ::Array parameters
function add_arrays(x::Array, y::Array)
    return x + y
end

# isapprox implementation with ::Array parameters (from Issue #662)
function isapprox_typed(x::Array, y::Array)
    rtol = 1.4901161193847656e-8
    atol = 0.0
    diff_norm = norm(x - y)
    max_norm = max(norm(x), norm(y))
    return diff_norm <= max(atol, rtol * max_norm)
end

# Test vectors
x = [1.0, 2.0, 3.0]
y = [4.0, 5.0, 6.0]
z = [1.0, 2.0, 3.0]  # Same as x

# Run tests
test1 = sum_array(x) == 6.0
test2 = dot_arrays(x, y) == 32.0  # 1*4 + 2*5 + 3*6 = 4+10+18 = 32
test3 = add_arrays(x, y) == [5.0, 7.0, 9.0]
test4 = isapprox_typed(x, z)  # Same vectors should be approximately equal
test5 = !isapprox_typed(x, y)  # Different vectors should not be approximately equal

# Return true only if all tests pass
result = test1 && test2 && test3 && test4 && test5
