# =============================================================================
# Sort - Sorting algorithms and utilities
# =============================================================================
# Based on Julia's base/sort.jl
#
# Note: Julia uses algorithm types (InsertionSort, QuickSort, etc.) but
# SubsetJuliaVM uses a simplified implementation with inline algorithms.
# The public API (sort!, sort, etc.) matches Julia's interface.

# =============================================================================
# Sorting Algorithm Types
# =============================================================================
# Based on Julia's base/sort.jl
#
# These types are used to select sorting algorithms. In SubsetJuliaVM,
# we provide the types for compatibility, but the actual implementation
# uses a simplified algorithm internally.

"""
    Algorithm

Abstract type for sorting algorithm selector types.
"""
abstract type Algorithm end

"""
    InsertionSortAlg <: Algorithm

Insertion sort algorithm type. Stable sort with O(n²) complexity.
Good for small arrays or nearly sorted data.
"""
struct InsertionSortAlg <: Algorithm end

"""
    InsertionSort

Indicate that a sorting function should use the insertion sort
algorithm. Insertion sort traverses the collection one element
at a time, inserting each element into its correct, sorted position in
the output vector.

Characteristics:
  * *stable*: preserves the ordering of elements that compare equal.
  * *in-place* in memory.
  * *quadratic performance* in the number of elements to be sorted.

Note that `InsertionSort` is `O(n²)` but has a smaller constant factor
than `QuickSort` or `MergeSort`, making it faster for small arrays.
"""
const InsertionSort = InsertionSortAlg()

"""
    QuickSortAlg <: Algorithm

Quick sort algorithm type. Unstable sort with O(n log n) average complexity.
"""
struct QuickSortAlg <: Algorithm end

"""
    QuickSort

Indicate that a sorting function should use the quick sort
algorithm, which is *not* stable.

Characteristics:
  * *not stable*: does not preserve the ordering of elements that compare equal.
  * *in-place* in memory.
  * *divide-and-conquer*: sort strategy similar to [`MergeSort`](@ref).
  * *good performance* for large collections.
"""
const QuickSort = QuickSortAlg()

"""
    MergeSortAlg <: Algorithm

Merge sort algorithm type. Stable sort with O(n log n) complexity.
"""
struct MergeSortAlg <: Algorithm end

"""
    MergeSort

Indicate that a sorting function should use the merge sort
algorithm. Merge sort divides the collection into
subcollections and repeatedly merges them, sorting each
subcollection at each step, until the entire
collection has been recombined in sorted form.

Characteristics:
  * *stable*: preserves the ordering of elements that compare equal.
  * *not in-place* in memory.
  * *divide-and-conquer*: sort strategy similar to [`QuickSort`](@ref).
  * *good performance* for large collections but requires more memory
    than `QuickSort`.
"""
const MergeSort = MergeSortAlg()

"""
    PartialQuickSort{T} <: Algorithm

Partial quick sort algorithm type. Only sorts enough to find the k-th
smallest elements.
"""
struct PartialQuickSort <: Algorithm
    k::Int64
end

# =============================================================================
# Public API
# =============================================================================

# sort!: in-place sort using insertion sort algorithm
# Supports keyword arguments: lt (comparison), by (transform function), rev (reverse order)
# Based on Julia's base/sort.jl:1734
function sort!(v; lt=nothing, by=nothing, rev=false)
    n = length(v)
    for i in 2:n
        key = v[i]
        if by === nothing
            key_val = key
        else
            key_val = by(key)
        end
        j = i - 1
        # Use explicit break to avoid short-circuit && evaluation issue
        while j >= 1
            if by === nothing
                j_val = v[j]
            else
                j_val = by(v[j])
            end
            if lt === nothing
                if j_val <= key_val
                    break
                end
            else
                # lt(a, b) returns true if a should come before b
                # Continue shifting while lt(key_val, j_val) (key should come before j_val)
                # Break when key should NOT come before j_val
                if lt(key_val, j_val) == false
                    break
                end
            end
            v[j + 1] = v[j]
            j = j - 1
        end
        v[j + 1] = key
    end
    if rev == true
        reverse!(v)
    end
    return v
end

# sort: return a sorted copy (type-preserving)
# Supports keyword arguments: lt (comparison), by (transform function), rev (reverse order)
# Based on Julia's base/sort.jl:2114
function sort(v; lt=nothing, by=nothing, rev=false)
    result = collect(v)
    return sort!(result, lt=lt, by=by, rev=rev)
end

# =============================================================================
# Partial sorting
# =============================================================================

# partialsort!: partially sort array so that k-th element is in correct position
# Returns the k-th smallest element
function partialsort!(arr, k)
    n = length(arr)
    if k < 1 || k > n
        return arr[1]  # Error case
    end

    # Use selection sort approach for the first k elements
    for i in 1:k
        min_idx = i
        for j in (i+1):n
            if arr[j] < arr[min_idx]
                min_idx = j
            end
        end
        # Swap
        if min_idx != i
            temp = arr[i]
            arr[i] = arr[min_idx]
            arr[min_idx] = temp
        end
    end
    return arr[k]
end

# partialsort: return k-th smallest element without modifying input (type-preserving)
function partialsort(arr, k)
    copy = collect(arr)
    return partialsort!(copy, k)
end

# =============================================================================
# Permutation sorting
# =============================================================================

# sortperm: return permutation that would sort the array (returns Int64 array)
# Supports keyword arguments: by (transform function), rev (reverse order), lt (comparison)
# Based on Julia's base/sort.jl:1968
function sortperm(arr; by=nothing, rev=false, lt=nothing)
    n = length(arr)
    # Create Int64 array for indices using collect(1:n)
    perm = collect(1:n)
    return sortperm!(perm, arr, by=by, rev=rev, lt=lt)
end

# sortperm!: compute the permutation in-place in the given array
# Supports keyword arguments: by (transform function), rev (reverse order), lt (comparison)
# Based on Julia's base/sort.jl:2039
#
# Fills `perm` with the permutation indices that would sort `arr`.
# The array `perm` must have the same length as `arr`.
function sortperm!(perm, arr; by=nothing, rev=false, lt=nothing)
    n = length(arr)

    # Initialize perm to 1:n
    for i in 1:n
        perm[i] = i
    end

    # Insertion sort on indices based on arr values
    for i in 2:n
        key_idx = perm[i]
        if by === nothing
            key_val = arr[key_idx]
        else
            key_val = by(arr[key_idx])
        end
        j = i - 1
        # Use explicit break to avoid short-circuit && evaluation issue
        while j >= 1
            if by === nothing
                j_val = arr[perm[j]]
            else
                j_val = by(arr[perm[j]])
            end
            if lt === nothing
                if j_val <= key_val
                    break
                end
            else
                if lt(key_val, j_val) == false
                    break
                end
            end
            perm[j + 1] = perm[j]
            j = j - 1
        end
        perm[j + 1] = key_idx
    end
    if rev == true
        reverse!(perm)
    end
    return perm
end

# partialsortperm!: compute partial sort permutation in-place
# Returns perm filled with indices such that arr[perm[1:k]] are the k smallest elements
# in sorted order
function partialsortperm!(perm, arr, k)
    n = length(arr)

    # Initialize perm to 1:n
    for i in 1:n
        perm[i] = i
    end

    # Selection sort for the first k elements
    for i in 1:k
        min_idx = i
        for j in (i+1):n
            if arr[perm[j]] < arr[perm[min_idx]]
                min_idx = j
            end
        end
        if min_idx != i
            temp = perm[i]
            perm[i] = perm[min_idx]
            perm[min_idx] = temp
        end
    end
    return perm
end

# partialsortperm: return partial sort permutation without modifying input
function partialsortperm(arr, k)
    n = length(arr)
    perm = collect(1:n)
    return partialsortperm!(perm, arr, k)
end

# =============================================================================
# Array reversal
# =============================================================================

# reverse!: reverse array in place
function reverse!(arr)
    n = length(arr)
    half = div(n, 2)
    for i in 1:half
        j = n - i + 1
        temp = arr[i]
        arr[i] = arr[j]
        arr[j] = temp
    end
    return arr
end

# reverse! with range: reverse portion of array in place
# Based on Julia's base/abstractarray.jl
function reverse!(arr, lo::Int64, hi::Int64)
    # Swap elements from both ends moving toward center
    while lo < hi
        temp = arr[lo]
        arr[lo] = arr[hi]
        arr[hi] = temp
        lo = lo + 1
        hi = hi - 1
    end
    return arr
end

# =============================================================================
# Binary search utilities
# =============================================================================

# searchsortedfirst: find first index where value could be inserted
# Uses linear search to avoid VM scoping bug with if-assignments in while loops
function searchsortedfirst(arr, x)
    n = length(arr)
    for i in 1:n
        if arr[i] >= x
            return i
        end
    end
    return n + 1
end

# searchsortedlast: find last index where value could be inserted
# Uses linear search to avoid VM scoping bug with if-assignments in while loops
function searchsortedlast(arr, x)
    n = length(arr)
    result = 0
    for i in 1:n
        if arr[i] <= x
            result = i
        end
    end
    return result
end

# searchsorted: return range of indices where value could be inserted
# Based on Julia's base/sort.jl:316
#
# Returns the range of indices in arr where values are equivalent to x,
# or an empty range located at the insertion point if arr does not contain x.
function searchsorted(arr, x)
    first_idx = searchsortedfirst(arr, x)
    last_idx = searchsortedlast(arr, x)
    return first_idx:last_idx
end

# issorted: check if array is sorted
# Supports keyword arguments: by (transform function), rev (reverse order), lt (comparison)
# Based on Julia's base/sort.jl
function issorted(arr; by=nothing, rev=false, lt=nothing)
    n = length(arr)
    if n <= 1
        return true
    end
    for i in 1:(n-1)
        if by === nothing
            a_val = arr[i]
            b_val = arr[i+1]
        else
            a_val = by(arr[i])
            b_val = by(arr[i+1])
        end
        if rev == true
            # In reverse mode, check that sequence is non-increasing
            if lt === nothing
                if a_val < b_val
                    return false
                end
            else
                if lt(a_val, b_val) == true
                    return false
                end
            end
        else
            # In normal mode, check that sequence is non-decreasing
            if lt === nothing
                if a_val > b_val
                    return false
                end
            else
                if lt(b_val, a_val) == true
                    return false
                end
            end
        end
    end
    return true
end

# insorted: check if value exists in sorted array
# Based on Julia's base/sort.jl:429 — insorted(x, v)
# Note: Julia's signature is insorted(x, v) where x is the value and v is the sorted collection
function insorted(x, arr)
    idx = searchsortedfirst(arr, x)
    n = length(arr)
    # Avoid short-circuit && evaluation issue
    if idx <= n
        if arr[idx] == x
            return true
        end
    end
    return false
end
