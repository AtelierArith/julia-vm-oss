# Loop Invariant Code Motion (LICM) Examples
#
# This example demonstrates code patterns where the AoT compiler
# hoists loop-invariant computations outside the loop.
#
# Key features:
# - Computations that don't depend on loop variables
# - Pure function calls with constant arguments
# - Immutable variable references

"""
    scale_array(arr, scale::Float64)

Scale an array by a factor.

The sqrt(scale) is loop-invariant and can be hoisted.
"""
function scale_array(arr::Vector{Float64}, scale::Float64)
    n = length(arr)
    result = zeros(n)

    # sqrt(scale) is loop-invariant - computed every iteration
    # but can be hoisted by the optimizer
    for i in 1:n
        result[i] = arr[i] * sqrt(scale)
    end

    return result
end

"""
    scale_array_optimized(arr, scale::Float64)

Scale an array by a factor (manually optimized version).

Shows what LICM does: sqrt(scale) is computed once before the loop.
"""
function scale_array_optimized(arr::Vector{Float64}, scale::Float64)
    n = length(arr)
    result = zeros(n)

    # Compute invariant outside loop (manual optimization)
    factor = sqrt(scale)
    for i in 1:n
        result[i] = arr[i] * factor
    end

    return result
end

"""
    normalize_by_length(arr)

Normalize array elements by the array length.

The length(arr) call is loop-invariant.
"""
function normalize_by_length(arr::Vector{Float64})
    n = length(arr)
    result = zeros(n)

    # length(arr) is invariant - will be hoisted
    for i in 1:n
        result[i] = arr[i] / Float64(length(arr))
    end

    return result
end

"""
    apply_trigonometric_scale(arr, angle::Float64)

Scale array elements by sin and cos of a fixed angle.

Both sin(angle) and cos(angle) are loop-invariant.
"""
function apply_trigonometric_scale(arr::Vector{Float64}, angle::Float64)
    n = length(arr)
    result = zeros(n)

    # Both sin and cos are invariant
    for i in 1:n
        result[i] = arr[i] * sin(angle) + cos(angle)
    end

    return result
end

"""
    weighted_sum_invariant(data, weights, offset::Float64)::Float64

Compute weighted sum with invariant offset computation.

The sqrt(offset) * 2 computation is loop-invariant.
"""
function weighted_sum_invariant(data::Vector{Float64}, weights::Vector{Float64}, offset::Float64)::Float64
    n = length(data)
    total = 0.0

    # This expression doesn't depend on i - will be hoisted
    for i in 1:n
        total += data[i] * weights[i] + sqrt(offset) * 2.0
    end

    return total
end

"""
    matrix_row_scale(m, row_scale::Float64)

Scale each row of a matrix by a constant factor.

The inner sqrt computation is invariant relative to the inner loop.
"""
function main()
    arr = [1.0, 2.0, 3.0, 4.0]
    scale_array(arr, 4.0)
    scale_array_optimized(arr, 4.0)
    normalize_by_length(arr)
    apply_trigonometric_scale([1.0], 0.0)
    weighted_sum_invariant([1.0], [1.0], 1.0)
    true
end
