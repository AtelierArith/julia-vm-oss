# Loop Unrolling Examples
#
# This example demonstrates code patterns that benefit from loop unrolling.
# The AoT compiler unrolls loops with constant bounds.
#
# Key features:
# - Constant loop bounds (e.g., 1:4)
# - Small loop bodies
# - No early exits (break/continue)

"""
    sum_fixed_4()::Int64

Sum integers 1 to 4 using a constant-range loop.

This loop will be unrolled by the AoT compiler into:
    result = 0
    result += 1
    result += 2
    result += 3
    result += 4
"""
function sum_fixed_4()::Int64
    result = 0
    for i in 1:4
        result += i
    end
    return result
end

"""
    polynomial_eval(x::Float64)::Float64

Evaluate a polynomial using Horner's method with unrolled iterations.

Computes: 1 + 2x + 3x^2 + 4x^3

The loop with constant bounds will be unrolled.
"""
function polynomial_eval(x::Float64)::Float64
    # Coefficients: [1, 2, 3, 4]
    coeffs = [1.0, 2.0, 3.0, 4.0]
    result = 0.0

    # Horner's method: ((4*x + 3)*x + 2)*x + 1
    # Unroll-friendly version with constant iterations
    for i in 4:-1:1
        result = result * x + coeffs[i]
    end
    return result
end

"""
    vector_sum_unrolled(v::Vector{Float64})::Float64

Sum vector elements with manual 4-way unrolling for demonstration.

This pattern helps the optimizer recognize parallelism.
"""
function vector_sum_unrolled(v::Vector{Float64})::Float64
    n = length(v)
    sum1 = 0.0
    sum2 = 0.0
    sum3 = 0.0
    sum4 = 0.0

    # Process 4 elements at a time
    i = 1
    while i + 3 <= n
        sum1 += v[i]
        sum2 += v[i + 1]
        sum3 += v[i + 2]
        sum4 += v[i + 3]
        i += 4
    end

    # Handle remaining elements
    while i <= n
        sum1 += v[i]
        i += 1
    end

    return sum1 + sum2 + sum3 + sum4
end

function main()
    sum_fixed_4()
    polynomial_eval(1.0)
    vector_sum_unrolled([1.0, 2.0, 3.0, 4.0, 5.0])
    true
end
