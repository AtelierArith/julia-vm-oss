# =============================================================================
# generator.jl - Generator type and iterator traits
# =============================================================================
# Based on Julia's base/generator.jl
#
# The iterate protocol:
#   iterate(collection) -> (element, state) | nothing
#   iterate(collection, state) -> (element, state) | nothing
#
# Note: Builtin types (Array, Tuple, Range, String) use VM instructions
# for iteration (IterateFirst/IterateNext). This file only defines iterate
# methods for custom iterator wrapper types.

# =============================================================================
# Generator - lazy map over iterator
# =============================================================================
# Based on Julia's base/generator.jl
#
# Generator(f, iter) yields f(x) for each x in iter
# This is the underlying type for generator expressions: (f(x) for x in iter)
#
# Note: This Pure Julia implementation requires the field function call feature
# (Issue #1357) to call g.f(element) dynamically.

struct Generator
    f::Function
    iter
end

# Iterate protocol for Generator
# Returns (f(element), state) where element is from the inner iterator

function iterate(g::Generator)
    y = iterate(g.iter)
    if y === nothing
        return nothing
    end
    # Apply the function to the element, return (result, state)
    return (g.f(y[1]), y[2])
end

function iterate(g::Generator, state)
    y = iterate(g.iter, state)
    if y === nothing
        return nothing
    end
    # Apply the function to the element, return (result, state)
    return (g.f(y[1]), y[2])
end

function length(g::Generator)
    return length(g.iter)
end

# =============================================================================
# Iterator Size Traits
# =============================================================================
# Based on Julia's base/generator.jl:32-91
#
# IteratorSize specifies how to compute the size of an iterator.

"""
    IteratorSize

Abstract type for describing whether an iterator has a known size.
"""
abstract type IteratorSize end

"""
    HasLength()

Iterator has a known length (query with `length()`).
"""
struct HasLength <: IteratorSize end

"""
    HasShape{N}()

Iterator has a known shape (N-dimensional, query with `size()`).
"""
struct HasShape{N} <: IteratorSize end

"""
    SizeUnknown()

Iterator has unknown size (cannot be determined without iteration).
"""
struct SizeUnknown <: IteratorSize end

"""
    IsInfinite()

Iterator is infinite (never exhausts).
"""
struct IsInfinite <: IteratorSize end

# =============================================================================
# Iterator Element Type Traits
# =============================================================================
# Based on Julia's base/generator.jl:95-110
#
# IteratorEltype specifies whether an iterator's element type is known.

"""
    IteratorEltype

Abstract type for describing whether an iterator's element type is known.
"""
abstract type IteratorEltype end

"""
    HasEltype()

Iterator has a known element type (query with `eltype()`).
"""
struct HasEltype <: IteratorEltype end

"""
    EltypeUnknown()

Iterator's element type is unknown.
"""
struct EltypeUnknown <: IteratorEltype end
