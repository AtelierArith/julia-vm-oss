# Type Stability Examples
#
# This example demonstrates how to write type-stable code that compiles
# efficiently with the AoT compiler.
#
# Key concepts:
# - Consistent return types in all branches
# - Type-annotated function parameters
# - Avoiding type instability patterns

# ============================================================================
# Good Patterns: Type-Stable Code
# ============================================================================

"""
    safe_divide(a::Float64, b::Float64)::Float64

Type-stable division that returns 0.0 for division by zero.

Both branches return Float64, so the function is type-stable.
"""
function safe_divide(a::Float64, b::Float64)::Float64
    if b == 0.0
        return 0.0  # Float64
    else
        return a / b  # Float64
    end
end

"""
    clamp_value(x::Float64, min_val::Float64, max_val::Float64)::Float64

Clamp a value to a range. All paths return Float64.
"""
function clamp_value(x::Float64, min_val::Float64, max_val::Float64)::Float64
    if x < min_val
        return min_val
    elseif x > max_val
        return max_val
    else
        return x
    end
end

"""
    fibonacci_stable(n::Int64)::Int64

Type-stable Fibonacci that always returns Int64.
"""
function fibonacci_stable(n::Int64)::Int64
    if n <= 1
        return n  # Int64
    end
    a = 0
    b = 1
    for _ in 2:n
        temp = a + b
        a = b
        b = temp
    end
    return b  # Int64
end

"""
    find_max_index(arr)::Int64

Find the index of the maximum value.

Returns 0 for empty arrays (consistent Int64 return).
"""
function find_max_index(arr)::Int64
    n = length(arr)
    if n == 0
        return 0  # Int64
    end

    max_idx = 1
    max_val = arr[1]

    for i in 2:n
        if arr[i] > max_val
            max_val = arr[i]
            max_idx = i
        end
    end

    return max_idx  # Int64
end

# ============================================================================
# Container Type Stability
# ============================================================================

"""
    zeros_float64(n::Int64)

Create a type-stable zero vector.
"""
function zeros_float64(n::Int64)
    return zeros(n)
end

"""
    range_sum_float(start::Int64, stop::Int64)::Float64

Sum a range as Float64 (avoids integer overflow).
"""
function range_sum_float(start::Int64, stop::Int64)::Float64
    total = 0.0
    for i in start:stop
        total += Float64(i)
    end
    return total
end

# ============================================================================
# Accumulator Type Stability
# ============================================================================

"""
    weighted_average(values, weights)::Float64

Type-stable weighted average computation.

All intermediate values are Float64.
"""
function weighted_average(values::Vector{Float64}, weights::Vector{Float64})::Float64
    n = length(values)
    if n == 0
        return 0.0
    end

    sum_weighted = 0.0  # Float64 accumulator
    sum_weights = 0.0   # Float64 accumulator

    for i in 1:n
        sum_weighted += values[i] * weights[i]
        sum_weights += weights[i]
    end

    if sum_weights == 0.0
        return 0.0
    end

    return sum_weighted / sum_weights
end

function main()
    safe_divide(10.0, 2.0)
    clamp_value(5.0, 0.0, 10.0)
    fibonacci_stable(10)
    find_max_index([1.0, 3.0, 2.0])
    find_max_index(Float64[])
    weighted_average([1.0, 2.0, 3.0], [1.0, 1.0, 1.0])
    true
end
