# =============================================================================
# Tuple - Tuple utilities
# =============================================================================
# Based on Julia's base/tuple.jl
#
# IMPORTANT: This module only contains functions that exist in Julia Base.
#
# Removed functions (not in Julia Base with these names):
#   - ntuple_indices, ntuple_fill (use ntuple with lambda)
#   - tuple_reverse (use reverse)
#   - tuple_map_square, tuple_map_double (use map)
#   - tuple_sum, tuple_prod (use sum, prod)
#   - tuple_min, tuple_max (use minimum, maximum)
#   - tuple_contains (use in)
#   - tuple_index (use findfirst)
#   - tuple_count (use count)
#
# Note: Julia's tuple operations use standard functions like sum, prod, etc.
# that work on any iterable. No special tuple-specific functions are needed.

# =============================================================================
# copy(t::Tuple) - copy of a Tuple (identity, since Tuples are immutable)
# Reference: julia/base/tuple.jl
# =============================================================================

# Tuples are immutable in Julia, so copy simply returns the tuple itself.
# This matches Julia's behavior where copy on immutable types is identity.
copy(t::Tuple) = t

# =============================================================================
# first for Tuple
# =============================================================================
# Returns the first element of a tuple.
# Based on Julia's base/tuple.jl:269-270
#
# Examples:
#   first((1, 2, 3)) => 1
#   first((42,)) => 42
#   first(()) => throws ArgumentError

function first(t::Tuple)
    n = length(t)
    if n == 0
        throw(ArgumentError("tuple must be non-empty"))
    end
    return t[1]
end

# =============================================================================
# last for Tuple
# =============================================================================
# Returns the last element of a tuple.
# Based on Julia's base/tuple.jl (implicit from indexing)
#
# Examples:
#   last((1, 2, 3)) => 3
#   last((42,)) => 42
#   last(()) => throws ArgumentError

function last(t::Tuple)
    n = length(t)
    if n == 0
        throw(ArgumentError("tuple must be non-empty"))
    end
    return t[n]
end

# =============================================================================
# reverse for Tuple
# =============================================================================
# Returns a new tuple with elements in reverse order.
# Based on Julia's base/tuple.jl:644
#
# Since lambda expressions (i -> ...) are not supported in prelude,
# we implement fixed-size overloads for common tuple sizes.
#
# Examples:
#   reverse((1, 2, 3)) => (3, 2, 1)
#   reverse(()) => ()
#   reverse((42,)) => (42,)

# Tuple reverse - uses runtime dispatch via isa check
function reverse(t::Tuple)
    n = length(t)
    if n == 0
        return ()
    elseif n == 1
        return (t[1],)
    elseif n == 2
        return (t[2], t[1])
    elseif n == 3
        return (t[3], t[2], t[1])
    elseif n == 4
        return (t[4], t[3], t[2], t[1])
    elseif n == 5
        return (t[5], t[4], t[3], t[2], t[1])
    elseif n == 6
        return (t[6], t[5], t[4], t[3], t[2], t[1])
    elseif n == 7
        return (t[7], t[6], t[5], t[4], t[3], t[2], t[1])
    elseif n == 8
        return (t[8], t[7], t[6], t[5], t[4], t[3], t[2], t[1])
    else
        # Fallback for larger tuples: return as array (compatibility mode)
        result = collect(t)
        m = length(result)
        for i in 1:div(m, 2)
            tmp = result[i]
            result[i] = result[m - i + 1]
            result[m - i + 1] = tmp
        end
        return result
    end
end

# =============================================================================
# front for Tuple
# =============================================================================
# Returns a tuple containing all but the last element.
# Based on Julia's base/tuple.jl:339
#
# Examples:
#   front((1, 2, 3)) => (1, 2)
#   front((1, 2)) => (1,)
#   front((1,)) => ()
#   front(()) => throws ArgumentError

function front(t::Tuple)
    n = length(t)
    if n == 0
        throw(ArgumentError("Cannot call front on an empty tuple."))
    elseif n == 1
        return ()
    elseif n == 2
        return (t[1],)
    elseif n == 3
        return (t[1], t[2])
    elseif n == 4
        return (t[1], t[2], t[3])
    elseif n == 5
        return (t[1], t[2], t[3], t[4])
    elseif n == 6
        return (t[1], t[2], t[3], t[4], t[5])
    elseif n == 7
        return (t[1], t[2], t[3], t[4], t[5], t[6])
    elseif n == 8
        return (t[1], t[2], t[3], t[4], t[5], t[6], t[7])
    else
        # Fallback for larger tuples: return as array
        result = collect(t)
        pop!(result)
        return result
    end
end

# =============================================================================
# tail for Tuple
# =============================================================================
# Returns a tuple containing all but the first element.
# Based on Julia's base/essentials.jl:534
#
# This is the converse of front: tail skips the first entry,
# while front skips the last entry.
#
# Examples:
#   tail((1, 2, 3)) => (2, 3)
#   tail((1, 2)) => (2,)
#   tail((1,)) => ()
#   tail(()) => throws ArgumentError

function tail(t::Tuple)
    n = length(t)
    if n == 0
        throw(ArgumentError("Cannot call tail on an empty tuple."))
    elseif n == 1
        return ()
    elseif n == 2
        return (t[2],)
    elseif n == 3
        return (t[2], t[3])
    elseif n == 4
        return (t[2], t[3], t[4])
    elseif n == 5
        return (t[2], t[3], t[4], t[5])
    elseif n == 6
        return (t[2], t[3], t[4], t[5], t[6])
    elseif n == 7
        return (t[2], t[3], t[4], t[5], t[6], t[7])
    elseif n == 8
        return (t[2], t[3], t[4], t[5], t[6], t[7], t[8])
    else
        # Fallback for larger tuples: return as array
        result = collect(t)
        popfirst!(result)
        return result
    end
end

# =============================================================================
# safe_tail for Tuple
# =============================================================================
# Version of tail that doesn't throw on empty tuples.
# Based on Julia's base/tuple.jl:318-319
#
# Used internally for array indexing and other operations where
# an empty tuple should silently return empty tuple.
#
# Examples:
#   safe_tail((1, 2, 3)) => (2, 3)
#   safe_tail((1,)) => ()
#   safe_tail(()) => ()  # Unlike tail, doesn't throw

function safe_tail(t::Tuple)
    n = length(t)
    if n == 0
        return ()  # Safe: returns empty tuple instead of throwing
    else
        return tail(t)
    end
end
