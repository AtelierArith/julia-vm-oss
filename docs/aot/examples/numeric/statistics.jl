# Statistical Functions - Type-Stable Numeric Computation
#
# This example demonstrates statistical computations with type stability.
#
# Key features:
# - Consistent Float64 return types
# - Type-annotated parameters
# - Efficient single-pass algorithms where possible

function mean(arr::Vector{Float64})::Float64
    n = length(arr)
    if n == 0
        return 0.0
    end
    total = 0.0
    for x in arr
        total += x
    end
    return total / Float64(n)
end

function variance(arr::Vector{Float64})::Float64
    n = length(arr)
    if n == 0
        return 0.0
    end

    # First pass: compute mean
    m = mean(arr)

    # Second pass: compute sum of squared deviations
    sum_sq = 0.0
    for x in arr
        diff = x - m
        sum_sq += diff * diff
    end

    return sum_sq / Float64(n)
end

function std(arr::Vector{Float64})::Float64
    return sqrt(variance(arr))
end

function min_max(arr::Vector{Float64})
    n = length(arr)
    if n == 0
        return (0.0, 0.0)
    end

    min_val = arr[1]
    max_val = arr[1]

    for i in 2:n
        x = arr[i]
        if x < min_val
            min_val = x
        end
        if x > max_val
            max_val = x
        end
    end

    return (min_val, max_val)
end

function normalize(arr::Vector{Float64})
    n = length(arr)
    m = mean(arr)
    s = std(arr)

    if s == 0.0
        # All values are the same, return zeros
        return zeros(n)
    end

    result = zeros(n)
    for i in 1:n
        result[i] = (arr[i] - m) / s
    end
    return result
end

function main()
    xs = [1.0, 2.0, 3.0, 4.0, 5.0]
    m = mean(xs)
    v = variance(xs)
    s = std(xs)
    (mn, mx) = min_max(xs)
    z = normalize(xs)
    m
    v
    s
    mn
    mx
    z[1]
    true
end
