# =============================================================================
# Set - Array utility functions and Set operations
# =============================================================================
# Based on Julia's base/set.jl and base/abstractset.jl
#
# Phase 4-3 (Issue #2574): Core Set operations via internal intrinsics
# Phase 4-4 (Issue #2575): Set algebra operations in Pure Julia
#
# Internal intrinsics (Rust-backed):
#   _set_push!(s, x)   - HashSet insert
#   _set_delete!(s, x)  - HashSet remove
#   _set_in(x, s)       - HashSet contains
#   _set_empty!(s)       - HashSet clear
#   _set_length(s)       - HashSet len

# =============================================================================
# Core Set operations - Pure Julia wrappers over internal intrinsics
# Reference: julia/base/set.jl
# =============================================================================

push!(s::Set, x) = _set_push!(s, x)
delete!(s::Set, x) = _set_delete!(s, x)
# Note: in(x, s::Set) is handled by the compiler/VM directly (BuiltinId::SetIn)
# because `in` is a keyword and cannot be used as a function name in the parser.
# The _set_in intrinsic is available for internal use.
empty!(s::Set) = _set_empty!(s)
length(s::Set) = _set_length(s)

# =============================================================================
# Set algebra operations - Pure Julia (Issue #2575)
# Reference: julia/base/abstractset.jl
# =============================================================================

# union(s1::Set, s2::Set) - set union
# Reference: julia/base/abstractset.jl:18
function union(s1::Set, s2::Set)
    result = Set()
    for x in s1
        push!(result, x)
    end
    for x in s2
        push!(result, x)
    end
    return result
end

# intersect(s1::Set, s2::Set) - set intersection
# Reference: julia/base/abstractset.jl:157
function intersect(s1::Set, s2::Set)
    result = Set()
    for x in s1
        if x in s2
            push!(result, x)
        end
    end
    return result
end

# setdiff(s1::Set, s2::Set) - set difference
# Reference: julia/base/abstractset.jl:277
function setdiff(s1::Set, s2::Set)
    result = Set()
    for x in s1
        if !(x in s2)
            push!(result, x)
        end
    end
    return result
end

# symdiff(s1::Set, s2::Set) - symmetric difference
# Reference: julia/base/abstractset.jl:318
function symdiff(s1::Set, s2::Set)
    result = Set()
    for x in s1
        if !(x in s2)
            push!(result, x)
        end
    end
    for x in s2
        if !(x in s1)
            push!(result, x)
        end
    end
    return result
end

# issubset(a::Set, b::Set) - subset check (a ⊆ b)
# Reference: julia/base/abstractset.jl:368
function issubset(a::Set, b::Set)
    for x in a
        if !(x in b)
            return false
        end
    end
    return true
end

# isdisjoint(a::Set, b::Set) - disjoint check
# Reference: julia/base/abstractset.jl:388
function isdisjoint(a::Set, b::Set)
    for x in a
        if x in b
            return false
        end
    end
    return true
end

# issetequal(a::Set, b::Set) - set equality
# Reference: julia/base/abstractset.jl:500
function issetequal(a::Set, b::Set)
    length(a) == length(b) && issubset(a, b)
end

# =============================================================================
# In-place Set operations - Pure Julia (Issue #2575)
# Reference: julia/base/abstractset.jl
# =============================================================================

# union!(s::Set, itr) - add all elements from itr to s
# Reference: julia/base/abstractset.jl:56
function union!(s::Set, itr)
    for x in itr
        push!(s, x)
    end
    return s
end

# intersect!(s::Set, itr) - keep only elements also in itr
# Reference: julia/base/abstractset.jl:172
function intersect!(s::Set, itr::Set)
    for x in s
        if !(x in itr)
            delete!(s, x)
        end
    end
    return s
end

# setdiff!(s::Set, itr) - remove elements found in itr
# Reference: julia/base/abstractset.jl:293
function setdiff!(s::Set, itr)
    for x in itr
        delete!(s, x)
    end
    return s
end

# symdiff!(s::Set, itr::Set) - symmetric difference in-place
# Reference: julia/base/abstractset.jl:341
function symdiff!(s::Set, itr::Set)
    for x in itr
        if x in s
            delete!(s, x)
        else
            push!(s, x)
        end
    end
    return s
end

# =============================================================================
# Array utility functions (set-like operations on arrays)
# =============================================================================

# unique: return array with duplicate elements removed
function unique(arr)
    n = length(arr)
    if n == 0
        return zeros(0)
    end

    # First pass: count unique elements
    count = 0
    for i in 1:n
        is_unique = true
        for j in 1:(i-1)
            if arr[i] == arr[j]
                is_unique = false
                break
            end
        end
        if is_unique
            count = count + 1
        end
    end

    # Second pass: build result array
    result = zeros(count)
    idx = 1
    for i in 1:n
        is_unique = true
        for j in 1:(i-1)
            if arr[i] == arr[j]
                is_unique = false
                break
            end
        end
        if is_unique
            result[idx] = arr[i]
            idx = idx + 1
        end
    end
    return result
end

# unique(f, itr): return elements from itr unique by f(x)
# Based on Julia's base/set.jl:301
# Returns elements of itr for which f(x) is unique (keeps first occurrence).
function unique(f::Function, arr)
    n = length(arr)
    if n == 0
        return zeros(0)
    end

    # First pass: count unique elements by f-value
    count = 0
    for i in 1:n
        is_unique = true
        for j in 1:(i-1)
            if f(arr[i]) == f(arr[j])
                is_unique = false
                break
            end
        end
        if is_unique
            count = count + 1
        end
    end

    # Second pass: build result array with original values
    result = zeros(count)
    idx = 1
    for i in 1:n
        is_unique = true
        for j in 1:(i-1)
            if f(arr[i]) == f(arr[j])
                is_unique = false
                break
            end
        end
        if is_unique
            result[idx] = arr[i]
            idx = idx + 1
        end
    end
    return result
end

# allunique: check if all elements in array are unique
function allunique(arr)
    n = length(arr)
    for i in 1:n
        for j in (i+1):n
            if arr[i] == arr[j]
                return false
            end
        end
    end
    return true
end

# allequal: check if all elements in array are equal
function allequal(arr)
    n = length(arr)
    if n <= 1
        return true
    end
    first = arr[1]
    for i in 2:n
        if arr[i] != first
            return false
        end
    end
    return true
end

# =============================================================================
# unique! - Remove duplicate elements in-place
# =============================================================================
# Based on Julia's base/set.jl:470
#
# unique!(A) removes duplicate elements from A in-place, preserving order.
# Returns the modified array.

function unique!(arr)
    n = length(arr)
    if n <= 1
        return arr
    end

    # j is the position where we write the next unique element
    j = 1

    for i in 2:n
        # Check if arr[i] is already in the "seen" part (arr[1:j])
        is_duplicate = false
        for k in 1:j
            if arr[i] == arr[k]
                is_duplicate = true
                break
            end
        end

        # If not a duplicate, keep it
        if !is_duplicate
            j = j + 1
            if j != i
                arr[j] = arr[i]
            end
        end
    end

    # Resize to keep only unique elements
    resize!(arr, j)
    return arr
end

# =============================================================================
# in! - Check membership and insert if not present
# =============================================================================
# Based on Julia's base/set.jl:125
#
# in!(x, s) checks if x is in s. If not, it inserts x and returns false.
# If x is already in s, returns true without modifying s.

function in!(x, s::Set)
    if x in s
        return true
    else
        push!(s, x)
        return false
    end
end

# =============================================================================
# copy(s::Set) - shallow copy of a Set
# Reference: julia/base/set.jl line 166
# =============================================================================

# copy(s::Set) = copymutable(s)
# In official Julia, copymutable creates a new Set from s's elements.
# We use union(s, Set()) which produces a shallow copy by unioning
# all elements of s into a new empty Set.
copy(s::Set) = union(s, Set())

# =============================================================================
# empty(s::Set) - create empty Set of same type
# Reference: julia/base/set.jl line 126
# =============================================================================

# empty(s::AbstractSet{T}, ::Type{U}=T) where {T,U} = Set{U}()
# Creates an empty Set. Type parameter handling is simplified here.
empty(s::Set) = Set()
