# =============================================================================
# Pair - Key-value pair type
# =============================================================================
# Based on Julia's base/pair.jl
#
# In official Julia, Pair is a parametric type Pair{A,B} with type parameters.
# In SubsetJuliaVM, we provide a simpler unparametrized version that stores
# first and second as Any types.

# =============================================================================
# Pair Type
# =============================================================================

"""
    Pair(first, second)
    Pair(first => second)

Construct a `Pair` object with type `Pair`.

The two elements are stored in the fields `first` and `second`.
They can also be accessed via iteration and indexing.

See also [`=>`](@ref).

# Examples
```julia
julia> p = Pair(1, 2)
1 => 2

julia> p.first
1

julia> p.second
2

julia> Pair("foo", 42)
"foo" => 42
```

Note: In official Julia, Pair is parametric (`Pair{A,B}`). SubsetJuliaVM uses
a simplified version where `first` and `second` can be any type.
"""
struct Pair
    first
    second
end

convert(::Type{Pair}, p::Pair) = p
convert(::Type{Pair{K,V}}, p::Pair) where {K,V} = p

length(p::Pair) = 2
eltype(p::Pair) = typejoin(typeof(p.first), typeof(p.second))
eltype(::Type{Pair}) = Any
eltype(::Type{Pair{K,V}}) where {K,V} = typejoin(K, V)
IteratorSize(::Type{<:Pair}) = HasLength()
IteratorEltype(::Type{<:Pair}) = HasEltype()

function iterate(p::Pair)
    return (p.first, 2)
end

function iterate(p::Pair, state)
    if state == 2
        return (p.second, 3)
    end
    return nothing
end

function collect(p::Pair)
    T = eltype(p)
    result = Vector{T}(undef, 2)
    result[1] = p.first
    result[2] = p.second
    return result
end

# Numeric indexing: p[1] == p.first, p[2] == p.second (mirrors Julia's Pair iteration)
function getindex(p::Pair, i::Int64)
    if i == 1
        return p.first
    elseif i == 2
        return p.second
    else
        error("BoundsError: attempt to access Pair at index $i")
    end
end
