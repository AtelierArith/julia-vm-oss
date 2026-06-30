# Issue #3520: Field access narrowing — obj.field should be inferred
# from refined type after `obj.field !== nothing`
mutable struct Boxed
    value::Union{Int64, Nothing}
end

function f(b::Boxed)
    if b.value !== nothing
        # In then branch: b.value should be inferred as Int64
        return b.value + 1
    end
    return 0
end

@assert f(Boxed(41)) == 42
@assert f(Boxed(nothing)) == 0

# isa narrowing on field
function g(b::Boxed)
    if b.value isa Int64
        return b.value + 1
    end
    return -1
end

@assert g(Boxed(41)) == 42
@assert g(Boxed(nothing)) == -1

# Issue #3716: getfield(obj, :field) narrows through the same path refinement
# key as obj.field.
function h(b::Boxed)
    if getfield(b, :value) !== nothing
        return getfield(b, :value) + 1
    end
    return 0
end

@assert h(Boxed(41)) == 42
@assert h(Boxed(nothing)) == 0

function i(b::Boxed)
    if isa(getfield(b, :value), Int64)
        return getfield(b, :value) + 1
    end
    return -1
end

@assert i(Boxed(41)) == 42
@assert i(Boxed(nothing)) == -1

true
