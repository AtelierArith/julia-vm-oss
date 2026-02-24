# =============================================================================
# genericmemory.jl - Memory{T} typed memory buffer
# =============================================================================
# Based on Julia's base/genericmemory.jl
#
# In Julia 1.11+, Memory{T} is an alias for GenericMemory{:not_atomic, T, Core.CPU}.
# It is a low-level fixed-size typed buffer used internally by Vector, Dict, etc.
#
# SubsetJuliaVM implementation: Memory{T} is a native Rust primitive type.
# Constructor: Memory{T}(n) creates a typed buffer of length n.
# Builtin support: length, size, getindex, setindex!, similar are handled natively.
#
# This file provides Pure Julia functions that work on top of the native Memory type:
# - copy(m::Memory): shallow copy

# =============================================================================
# Copy
# =============================================================================

"""
    copy(m::Memory)

Create a shallow copy of `m`.
"""
function copy(m::Memory)
    n = length(m)
    result = Memory{Any}(n)
    for i in 1:n
        result[i] = m[i]
    end
    return result
end
