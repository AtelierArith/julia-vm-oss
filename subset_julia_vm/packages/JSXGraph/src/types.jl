struct JSXElement
    id::Int
    type_name::Symbol
    parents
    attrs::Vector{Pair{Symbol,Any}}
end

struct JSFunction
    code::String
    var::Symbol
    var2::Symbol
end

struct View3D
    id::Int
    parents
    attrs::Vector{Pair{Symbol,Any}}
    elements::Vector{Any}
end

struct Board
    elements::Vector{Any}
    options::Vector{Pair{Symbol,Any}}
end

const _NEXT_ID = Int[0]

function _new_id()
    _NEXT_ID[1] += 1
    return _NEXT_ID[1]
end

function _append_kwargs!(pairs, kwargs)
    if kwargs === nothing
        return pairs
    end
    for (k, v) in kwargs
        push!(pairs, k => v)
    end
    return pairs
end

function _kw_to_attrs(kwargs)
    attrs = Pair{Symbol,Any}[]
    _append_kwargs!(attrs, kwargs)
    return attrs
end

# Single-argument form (curve3d's `t => ...`). The empty `var2` marks it as
# having only one parameter; the renderer falls back to a one-argument function.
_jsf(s::AbstractString) = JSFunction(s, :t, Symbol(""))
_jsf(f::JSFunction) = f

# Two-argument form for parametric surfaces: `(u, v) => ...`.
_jsf2(s::AbstractString) = JSFunction(s, :u, :v)
_jsf2(f::JSFunction) = f
