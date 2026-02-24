# =============================================================================
# Combinatorics - Combinatorial functions
# =============================================================================
# Based on Julia's base/combinatorics.jl
#
# IMPORTANT: This module only contains functions that exist in Julia Base.
# Removed functions (not in Julia Base):
#   - nPr (use factorial(n)÷factorial(n-k))
#   - nCr (use binomial(n, k))
#   - catalan (not in Base)
#   - fibonacci (not in Base)
#   - lucas (not in Base)
#   - stirling2 (not in Base)
#   - bell (not in Base)

# =============================================================================
# Factorials
# =============================================================================

"""
    factorial(n::Integer)

Factorial of `n`. If `n` is an [`Integer`](@ref), the factorial is computed as an
integer (promoted to at least 64 bits). Note that this may overflow if `n` is not small,
but you can use `factorial(big(n))` to compute the result exactly in arbitrary precision.
"""
function factorial(n)
    if n < 0
        error("factorial not defined for negative values")
    end
    if n > 20
        error("factorial($n) overflows; consider using factorial(big($n))")
    end
    result = 1
    for i in 2:n
        result = result * i
    end
    return result
end

# binomial: binomial coefficient C(n, k) = n! / (k! * (n-k)!)
# Uses the multiplicative formula for efficiency
function binomial(n, k)
    if k < 0 || k > n
        return 0
    end
    if k == 0 || k == n
        return 1
    end
    # Use symmetry: C(n,k) = C(n, n-k)
    if k > n - k
        k = n - k
    end
    result = 1
    i = 1
    while i <= k
        result = div(result * (n - k + i), i)
        i = i + 1
    end
    return result
end

# =============================================================================
# Permutation functions
# =============================================================================

"""
    isperm(v) -> Bool

Return `true` if `v` is a valid permutation.

A valid permutation is a vector containing each integer from 1 to length(v)
exactly once.

# Examples
```julia
julia> isperm([1, 2, 3])
true

julia> isperm([2, 1, 3])
true

julia> isperm([1, 3])
false

julia> isperm([1, 1, 2])
false
```
"""
function isperm(p)
    n = length(p)
    # Track which indices have been seen
    used = zeros(n)
    for i in 1:n
        v = p[i]
        # Check if v is a valid integer in range
        if v < 1 || v > n
            return false
        end
        # Check if v is an integer (not a float with decimal part)
        if v != floor(v)
            return false
        end
        vi = Int64(v)
        # Check if already used
        if used[vi] == 1.0
            return false
        end
        used[vi] = 1.0
    end
    return true
end

"""
    invperm(v)

Return the inverse permutation of `v`.

If `v` is a permutation of `1:n` such that `v[i] = j`, then
`invperm(v)[j] = i`.

# Examples
```julia
julia> v = [2, 4, 3, 1]
4-element Vector{Int64}:
 2
 4
 3
 1

julia> invperm(v)
4-element Vector{Int64}:
 4
 1
 3
 2

julia> A = ['a', 'b', 'c', 'd']
julia> B = A[v]  # ['b', 'd', 'c', 'a']
julia> B[invperm(v)]  # ['a', 'b', 'c', 'd'] (original order)
```
"""
function invperm(p)
    n = length(p)
    # Verify it's a valid permutation
    if !isperm(p)
        error("argument is not a permutation")
    end
    # Create inverse
    result = zeros(n)
    for i in 1:n
        j = Int64(p[i])
        result[j] = Float64(i)
    end
    return result
end

"""
    permute!(v, p)

Permute vector `v` in-place according to permutation `p`.

If `p` is a permutation of `1:n`, then after `permute!(v, p)`,
the element that was at index `p[i]` is now at index `i`.

# Examples
```julia
julia> A = [1, 1, 3, 4];
julia> perm = [2, 4, 3, 1];
julia> permute!(A, perm);
julia> A
4-element Vector{Int64}:
 1
 4
 3
 1
```
"""
function permute!(v, p)
    n = length(v)
    # Create a temporary copy
    temp = collect(v)
    # Apply permutation: v[i] = temp[p[i]]
    for i in 1:n
        v[i] = temp[Int64(p[i])]
    end
    return v
end

"""
    invpermute!(v, p)

Permute vector `v` in-place according to the inverse of permutation `p`.

The inverse of permutation `p` maps position `p[i]` to position `i`.
After `invpermute!(v, p)`, the element that was at index `i` is now at index `p[i]`.

# Examples
```julia
julia> A = [1, 1, 3, 4];
julia> perm = [2, 4, 3, 1];
julia> invpermute!(A, perm);
julia> A
4-element Vector{Int64}:
 4
 1
 3
 1
```
"""
function invpermute!(v, p)
    n = length(v)
    # Create a temporary copy
    temp = collect(v)
    # Apply inverse permutation: v[p[i]] = temp[i]
    for i in 1:n
        v[Int64(p[i])] = temp[i]
    end
    return v
end
