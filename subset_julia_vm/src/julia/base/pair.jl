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

