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
#
# Parametrized as upstream `Base.Generator{I,F}` (Issue #9200 slice 1): `iter::I`
# is the wrapped iterator, `f::F` the mapping function. Field ORDER matches
# upstream (`f` first, `iter` second) so the native `Value::Generator`
# projection (`generator_projected_field_by_index`: 0 -> f, 1 -> iter) and the
# `typeof` spelling `Base.Generator{I, F}` (iter first, callable second) stay in
# lock-step. Every `Generator(...)` surface call is intercepted by the compiler
# (`BuiltinOp::Generator`) and produces a native `Value::Generator`, so the
# struct's auto-generated constructor is never dispatched at runtime — the
# declaration exists to register the `Base.Generator` type for method dispatch
# (`iterate`/`size`/`IteratorSize`/... on `::Generator`) and its `{I,F}` params
# for the type-level iterator traits below.
struct Generator{I, F}
    f::F
    iter::I
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

# `isempty` of a generator must drive the iterate protocol, NOT `length`
# (Issue #9320). Upstream models a filtered generator as
# `Generator(map, Iterators.Filter(pred, iter))` and defines
# `isempty(g::Generator) = isempty(g.iter)`; for the filtered case `g.iter` is
# the `Filter`, whose `IteratorSize` is `SizeUnknown()`, so `isempty` reaches
# the iterate-based generic `isempty(itr) = iterate(itr) === nothing`
# (julia/base/essentials.jl) rather than `length`.
#
# sjulia collapses the filter into the generator's `callable` (Issue #9271), so
# `g.iter` here is the UNFILTERED base iterator; delegating to `isempty(g.iter)`
# would ignore the predicate (a fully filtered-out generator would wrongly
# report non-empty). It would also route through the length-based generic
# `isempty(arr) = length(arr) == 0` (range.jl), and `length` of a filtered
# generator is a MethodError (Issue #9320) — so isempty threw instead of
# returning a Bool. Drive `iterate(g)` directly: sjulia's generator iterate
# applies the predicate, so this reports emptiness correctly for both filtered
# and unfiltered generators and never touches `length`.
function isempty(g::Generator)
    return iterate(g) === nothing
end

function ndims(g::Generator)
    return ndims(g.iter)
end

function axes(g::Generator)
    return axes(g.iter)
end

# first(g::Generator) must apply the mapping, so it has to go through the
# iterate protocol — the generic `first(arr) = arr[1]` fallback in range.jl
# indexes the UNDERLYING iterator of a lazy generator and returns the raw
# element without applying `g.f` (Issue #9103). This mirrors upstream's
# generic `first(itr)` (julia/base/abstractarray.jl), which sjulia's generic
# fallback does not implement.
function first(g::Generator)
    y = iterate(g)
    y === nothing && throw(ArgumentError("collection must be non-empty"))
    return y[1]
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

# Type-level iterator-size trait, mirroring upstream
# `IteratorSize(::Type{<:Generator{I}}) where {I} = IteratorSize(I)`
# (julia/base/generator.jl). A `Generator{I,F}` delegates to the wrapped
# iterator type `I`, so e.g. `IteratorSize(typeof(x^2 for x in 1:5))` reports
# `HasShape{1}()` via `IteratorSize(UnitRange{Int64})`.
#
# For a native `Value::Generator` the VM currently answers this trait through a
# Rust fast path (`iterator_size_value_for_generator_iter_type_name`) that
# shadows this method, so this definition is dormant for the collapsed
# representation. It is added now as the upstream-shaped foundation the Filter
# desugar slice (#9200 S3) will rely on once the Rust special-cases retire.
function IteratorSize(::Type{Generator{I, F}}) where {I, F}
    return IteratorSize(I)
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

# Type-level iterator-eltype trait, mirroring upstream
# `IteratorEltype(::Type{Generator{I,T}}) where {I,T} = EltypeUnknown()`
# (julia/base/generator.jl). A generator's element type depends on the runtime
# mapping result, so it is never statically known.
function IteratorEltype(::Type{Generator{I, F}}) where {I, F}
    return EltypeUnknown()
end
