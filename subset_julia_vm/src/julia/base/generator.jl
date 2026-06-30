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
    f::Any
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

function size(g::Generator)
    return size(g.iter)
end

function ndims(g::Generator)
    return ndims(g.iter)
end

function axes(g::Generator)
    return axes(g.iter)
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

function IteratorSize(x)
    return IteratorSize(typeof(x))
end

function IteratorSize(::Type)
    return HasLength()
end

function IteratorSize(::Type{Any})
    return SizeUnknown()
end

function IteratorSize(t::Tuple)
    return HasLength()
end

function IteratorSize(a::Array)
    n = ndims(a)
    if n == 1
        return HasShape{1}()
    elseif n == 2
        return HasShape{2}()
    elseif n == 3
        return HasShape{3}()
    elseif n == 4
        return HasShape{4}()
    elseif n == 5
        return HasShape{5}()
    elseif n == 6
        return HasShape{6}()
    elseif n == 7
        return HasShape{7}()
    elseif n == 8
        return HasShape{8}()
    end
    return HasLength()
end

# Generic AbstractArray rule (Issue #8139). Upstream defines this purely at the
# type level as `IteratorSize(::Type{<:AbstractArray{<:Any,N}}) = HasShape{N}()`,
# but sjulia's dispatcher cannot bind the `N` parameter through the abstract
# supertype chain of non-`Array` arrays (e.g. StaticArrays'
# `SMatrix{2,2,Int64} <: ... <: AbstractArray{Int64,2}`) — even the plain
# `::Type{<:AbstractArray{T,N}}` form leaves `T`/`N` unbound. Expressing the same
# rule as a value-based method keyed on the runtime `ndims` recovers the shape
# and, crucially, still dispatches correctly when the value flows through a
# statically-`Any` parameter (the generic `collect(itr)` in iterators.jl).
# Without it, `collect(::StaticMatrix)` devirtualized to the generic
# `IteratorSize(::Type) = HasLength()` and flattened the 2-D shape to a `Vector`.
# `Array`/`AbstractRange`/`Memory` keep their own more-specific methods above.
function IteratorSize(a::AbstractArray)
    return HasShape{ndims(a)}()
end

function IteratorSize(::Type{Vector{T}}) where {T}
    return HasShape{1}()
end

function IteratorSize(::Type{Matrix{T}}) where {T}
    return HasShape{2}()
end

function IteratorSize(m::Memory)
    return HasShape{1}()
end

function IteratorSize(s::String)
    return HasLength()
end

function IteratorSize(r::AbstractRange)
    return HasShape{1}()
end

function IteratorSize(::Type{UnitRange{T}}) where {T}
    return HasShape{1}()
end

function IteratorSize(::Type{StepRange{T,S}}) where {T,S}
    return HasShape{1}()
end

function IteratorSize(g::Generator)
    return IteratorSize(g.iter)
end

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

function IteratorEltype(x)
    return IteratorEltype(typeof(x))
end

function IteratorEltype(::Type)
    return HasEltype()
end

function IteratorEltype(::Type{Any})
    return EltypeUnknown()
end

function IteratorEltype(t::Tuple)
    return HasEltype()
end

function IteratorEltype(a::Array)
    return HasEltype()
end

function IteratorEltype(::Type{Vector{T}}) where {T}
    return HasEltype()
end

function IteratorEltype(::Type{Matrix{T}}) where {T}
    return HasEltype()
end

function IteratorEltype(m::Memory)
    return HasEltype()
end

function IteratorEltype(s::String)
    return HasEltype()
end

function IteratorEltype(r::AbstractRange)
    return HasEltype()
end

function IteratorEltype(::Type{UnitRange{T}}) where {T}
    return HasEltype()
end

function IteratorEltype(::Type{StepRange{T,S}}) where {T,S}
    return HasEltype()
end

function IteratorEltype(g::Generator)
    return EltypeUnknown()
end
