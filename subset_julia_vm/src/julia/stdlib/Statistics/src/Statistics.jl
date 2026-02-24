# This file is a part of Julia. License is MIT: https://julialang.org/license

# =============================================================================
# Statistics - Standard library module for basic statistics functionality
# =============================================================================
# Strict subset of Julia's stdlib/Statistics
#
# IMPORTANT: This module only exports functions that exist in Julia's Statistics.jl
# with compatible signatures. Code that works here MUST work in Julia.
#
# Functions NOT included (belong to StatsBase.jl, not Statistics.jl):
# - mode, skewness, kurtosis, iqr, zscore
# - wmean, wvar, wstd (weighted statistics)
# - cov_matrix, cor_matrix (use cov/cor with matrix input in Julia)
#
# Keyword arguments (corrected=, dims=) are not yet supported.
# We implement only the default behavior (corrected=true, no dims).

module Statistics

export cor, cov, std, stdm, var, varm, mean, median, median!, middle, quantile, quantile!

# =============================================================================
# mean - Compute the arithmetic mean
# =============================================================================

# mean(itr) - Compute the mean of all elements in a collection
function mean(arr)
    n = length(arr)
    if n == 0
        return 0.0 / 0.0  # NaN for empty array
    end
    total = 0.0
    for i in 1:n
        total = total + arr[i]
    end
    return total / n
end

# mean(f, itr) - Apply function f to each element and take the mean
# Based on Julia's Statistics mean(f, itr)
function mean(f::Function, itr)
    n = length(itr)
    if n == 0
        return 0.0 / 0.0  # NaN for empty collection
    end
    total = 0.0
    for i in 1:n
        total = total + f(itr[i])
    end
    return total / n
end

# mean(arr; dims) - Compute mean along specified dimension
# Based on Julia's Statistics mean(A; dims)
# dims=0: global mean (default), dims=1: column means, dims=2: row means
function mean(arr; dims=0)
    if dims == 0
        n = length(arr)
        if n == 0
            return 0.0 / 0.0  # NaN for empty array
        end
        total = 0.0
        for i in 1:n
            total = total + arr[i]
        end
        return total / n
    end
    m = size(arr, 1)
    n = size(arr, 2)
    if dims == 1
        result = zeros(1, n)
        for j in 1:n
            s = 0.0
            for i in 1:m
                s = s + arr[i, j]
            end
            result[1, j] = s / m
        end
        return result
    elseif dims == 2
        result = zeros(m, 1)
        for i in 1:m
            s = 0.0
            for j in 1:n
                s = s + arr[i, j]
            end
            result[i, 1] = s / n
        end
        return result
    else
        error("mean: dims must be 1 or 2 for matrices")
    end
end

# =============================================================================
# var - Compute the sample variance
# =============================================================================

# var(itr) - Compute sample variance with Bessel's correction (n-1)
# Note: Julia's var has corrected=true by default, which we implement here
function var(arr)
    n = length(arr)
    if n <= 1
        return 0.0 / 0.0  # NaN for insufficient data
    end
    # Inline mean calculation to avoid compiler bug with for loop + outer scope variable
    total = 0.0
    for j in 1:n
        total = total + arr[j]
    end
    m = total / n
    sum_sq = 0.0
    for i in 1:n
        d = arr[i] - m
        sum_sq = sum_sq + d * d
    end
    return sum_sq / (n - 1)
end

# =============================================================================
# varm - Variance with known mean
# =============================================================================

# varm(itr, m) - Compute variance with pre-computed mean
# Note: Julia's varm has corrected=true by default
function varm(arr, m)
    n = length(arr)
    if n <= 1
        return 0.0 / 0.0
    end
    sum_sq = 0.0
    # Note: This pattern works because m is a parameter, not from outer scope via function call
    for i in 1:n
        d = arr[i] - m
        sum_sq = sum_sq + d * d
    end
    return sum_sq / (n - 1)
end

# =============================================================================
# std - Compute the sample standard deviation
# =============================================================================

# std(itr) - Standard deviation (sqrt of variance)
function std(arr)
    return sqrt(var(arr))
end

# =============================================================================
# stdm - Standard deviation with known mean
# =============================================================================

# stdm(itr, m) - Standard deviation with pre-computed mean
function stdm(arr, m)
    return sqrt(varm(arr, m))
end

# =============================================================================
# cov - Compute the covariance
# =============================================================================

# cov(x, y) - Covariance between two vectors
# Note: Julia's cov has corrected=true by default
function cov(x, y)
    n = length(x)
    if n != length(y)
        return 0.0 / 0.0  # Dimension mismatch
    end
    if n <= 1
        return 0.0 / 0.0
    end
    # Inline mean calculations to avoid compiler bug
    total_x = 0.0
    for j in 1:n
        total_x = total_x + x[j]
    end
    mx = total_x / n
    total_y = 0.0
    for j in 1:n
        total_y = total_y + y[j]
    end
    my = total_y / n
    sum_xy = 0.0
    for i in 1:n
        sum_xy = sum_xy + (x[i] - mx) * (y[i] - my)
    end
    return sum_xy / (n - 1)
end

# cov(x) - Variance of a single vector (same as var)
# This matches Julia's cov(x::AbstractVector)
function cov(x)
    return var(x)
end

# =============================================================================
# cor - Compute the Pearson correlation coefficient
# =============================================================================

# cor(x, y) - Correlation between two vectors
function cor(x, y)
    n = length(x)
    if n != length(y)
        return 0.0 / 0.0
    end
    if n <= 1
        return 0.0 / 0.0
    end
    # Inline mean calculations to avoid compiler bug
    total_x = 0.0
    for j in 1:n
        total_x = total_x + x[j]
    end
    mx = total_x / n
    total_y = 0.0
    for j in 1:n
        total_y = total_y + y[j]
    end
    my = total_y / n
    sum_xy = 0.0
    sum_xx = 0.0
    sum_yy = 0.0
    for i in 1:n
        dx = x[i] - mx
        dy = y[i] - my
        sum_xy = sum_xy + dx * dy
        sum_xx = sum_xx + dx * dx
        sum_yy = sum_yy + dy * dy
    end
    denom = sqrt(sum_xx * sum_yy)
    if denom == 0
        return 0.0 / 0.0
    end
    r = sum_xy / denom
    # Clamp to [-1, 1] to handle numerical errors
    if r > 1.0
        return 1.0
    elseif r < -1.0
        return -1.0
    else
        return r
    end
end

# cor(x) - Correlation of a vector with itself is 1
# This matches Julia's cor(x::AbstractVector)
function cor(x)
    n = length(x)
    if n == 0
        return 0.0 / 0.0
    end
    return 1.0
end

# =============================================================================
# middle - Compute the middle value
# =============================================================================

# middle(x, y) - Middle of two numbers (their mean)
function middle(x, y)
    return x / 2.0 + y / 2.0
end

# middle(a) - For a single value, return itself as Float64
# middle(a) - For an array, return mean of extrema
# Note: Julia uses multiple dispatch. We check if it's a number or array.
# Since we don't have full type dispatch, we implement array version separately
# and the scalar version is handled by the VM's numeric operations.

# =============================================================================
# median - Compute the median
# =============================================================================

# Helper: bubble sort for small arrays (in-place, internal use only)
function _sort_inplace!(arr)
    n = length(arr)
    for i in 1:(n-1)
        for j in 1:(n-i)
            if arr[j] > arr[j+1]
                temp = arr[j]
                arr[j] = arr[j+1]
                arr[j+1] = temp
            end
        end
    end
    return arr
end

# Helper: copy array (internal use only)
function _copy_array(arr)
    n = length(arr)
    result = zeros(n)
    for i in 1:n
        result[i] = arr[i]
    end
    return result
end

# median(arr) - Compute the median
function median(arr)
    n = length(arr)
    if n == 0
        return 0.0 / 0.0
    end
    # Copy and sort
    sorted = _copy_array(arr)
    _sort_inplace!(sorted)

    mid = Int64(floor(n / 2))
    if n % 2 == 1
        # Odd length: return middle element
        return sorted[mid + 1]
    else
        # Even length: return mean of two middle elements
        return middle(sorted[mid], sorted[mid + 1])
    end
end

# median!(v) - Compute the median, overwriting the input vector with sorted data
# Based on Julia's Statistics median!(v)
function median!(arr)
    n = length(arr)
    if n == 0
        return 0.0 / 0.0
    end
    # Sort in-place (no copy)
    _sort_inplace!(arr)

    mid = Int64(floor(n / 2))
    if n % 2 == 1
        return arr[mid + 1]
    else
        return middle(arr[mid], arr[mid + 1])
    end
end

# =============================================================================
# quantile - Compute quantiles
# =============================================================================

# quantile(arr, p) - Compute the p-th quantile (0 <= p <= 1)
# Uses linear interpolation (R/NumPy default, Definition 7)
# Note: Julia's quantile supports alpha/beta parameters, but we use defaults
function quantile(arr, p)
    if p < 0.0 || p > 1.0
        return 0.0 / 0.0  # Invalid probability
    end
    n = length(arr)
    if n == 0
        return 0.0 / 0.0
    end
    # Copy and sort
    sorted = _copy_array(arr)
    _sort_inplace!(sorted)

    if n == 1
        return sorted[1]
    end

    # Linear interpolation between points ((k-1)/(n-1), x[k])
    # This is Definition 7 (R/NumPy default)
    h = p * (n - 1)
    lo = Int64(floor(h)) + 1
    hi = lo + 1
    if hi > n
        return sorted[n]
    end
    if lo < 1
        return sorted[1]
    end

    frac = h - floor(h)
    return sorted[lo] + frac * (sorted[hi] - sorted[lo])
end

# quantile!(v, p) - Compute the p-th quantile, overwriting v with sorted data
# Based on Julia's Statistics quantile!(v, p)
function quantile!(arr, p)
    if p < 0.0 || p > 1.0
        return 0.0 / 0.0
    end
    n = length(arr)
    if n == 0
        return 0.0 / 0.0
    end
    # Sort in-place (no copy)
    _sort_inplace!(arr)

    if n == 1
        return arr[1]
    end

    h = p * (n - 1)
    lo = Int64(floor(h)) + 1
    hi = lo + 1
    if hi > n
        return arr[n]
    end
    if lo < 1
        return arr[1]
    end

    frac = h - floor(h)
    return arr[lo] + frac * (arr[hi] - arr[lo])
end

# quantile!(v, p::AbstractVector) - Compute quantiles for multiple probabilities, in-place
function quantile!(arr, ps::AbstractVector)
    n = length(arr)
    if n == 0
        result = zeros(length(ps))
        for i in 1:length(ps)
            result[i] = 0.0 / 0.0
        end
        return result
    end
    # Sort in-place once
    _sort_inplace!(arr)

    np = length(ps)
    result = zeros(np)
    for k in 1:np
        p = ps[k]
        if p < 0.0 || p > 1.0
            result[k] = 0.0 / 0.0
        elseif n == 1
            result[k] = arr[1]
        else
            h = p * (n - 1)
            lo = Int64(floor(h)) + 1
            hi = lo + 1
            if hi > n
                result[k] = arr[n]
            elseif lo < 1
                result[k] = arr[1]
            else
                frac = h - floor(h)
                result[k] = arr[lo] + frac * (arr[hi] - arr[lo])
            end
        end
    end
    return result
end

# quantile(arr, ps::AbstractVector) - Compute quantiles for multiple probabilities
# Non-destructive: copies the input array before sorting
function quantile(arr, ps::AbstractVector)
    sorted = _copy_array(arr)
    return quantile!(sorted, ps)
end

end # module Statistics
