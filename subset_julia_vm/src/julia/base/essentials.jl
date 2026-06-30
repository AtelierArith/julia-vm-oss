# =============================================================================
# essentials.jl - Essential language support functions
# =============================================================================
# Based on Julia's base/essentials.jl

# =============================================================================
# Bottom - the empty (bottom) type, alias for Union{}
# =============================================================================
# Based on Julia's base/essentials.jl: `const Bottom = Union{}`
#
# Union{} is the bottom of the type lattice: it is a subtype of every type,
# the zero element of typeintersect, and the empty-Union normal form. `Bottom`
# is the canonical name for it (Issue #5065).
const Bottom = Union{}

# =============================================================================
# ifelse - conditional without short-circuit evaluation
# =============================================================================
# Based on Julia's base/essentials.jl
#
# ifelse(condition, x, y) evaluates both x and y, returns x if true, y if false
# Unlike ternary operator, both branches are always evaluated

function ifelse(condition::Bool, x, y)
    if condition
        return x
    else
        return y
    end
end

# =============================================================================
# oftype - convert to type of reference value
# =============================================================================
# Based on Julia's base/essentials.jl
#
# oftype(x, y) converts y to the type of x, i.e. convert(typeof(x), y).
# Matches upstream: short-circuit when y is already a typeof(x), otherwise
# convert and assert the result type. (Issue #5109)
#
# The trailing `::typeof(x)` is a type assertion on a call expression, which
# now lowers to `typeassert(...)` (Issue #5193), so the exact upstream form is
# used verbatim.
oftype(x, y) = y isa typeof(x) ? y : convert(typeof(x), y)::typeof(x)

# =============================================================================
# Core language helpers
# =============================================================================
# Minimal compatibility definitions from Julia's base/essentials.jl.

function typeassert(x, T)
    if x isa T
        return x
    end
    # Upstream stores the offending VALUE in `TypeError.got` (boot.jl), not its
    # type; `showerror` later formats it as "a value of type $(typeof(got))"
    # (Issue #5146).
    throw(TypeError(:typeassert, "", T, x))
end

function cconvert(T::Type, x)
    return convert(T, x)
end

function unsafe_convert(T::Type, x)
    typeassert(x, T)
    return x
end

function unwrap_unionall(t)
    while isa(t, UnionAll)
        t = t.body
    end
    return t
end

function rewrap_unionall(t, u)
    if !isa(u, UnionAll)
        return t
    end
    return UnionAll(u.var, rewrap_unionall(t, u.body))
end

function isvarargtype(t)
    return isa(t, DataType) && nameof(t) === :Vararg
end

function isvatuple(t)
    t = unwrap_unionall(t)
    if isa(t, DataType)
        params = t.parameters
        n = length(params)
        return n > 0 && isvarargtype(params[n])
    end
    return false
end

# INTENTIONAL_NOOP (Issue #4703): upstream `donotdelete` is a compiler
# hint that prevents the optimizer from deleting effect-free expressions
# whose results are otherwise unused. sjulia's interpreter does not
# perform that DCE pass, so a `return nothing` body is the correct
# implementation, not a stub.
function donotdelete(x)
    return nothing
end

# INTENTIONAL_NOOP (Issue #4703): upstream `compilerbarrier(:type, x)` /
# `compilerbarrier(:const, x)` are inference barriers used by Julia's
# optimizer to discard type or constant information about `x`. sjulia
# does not perform the optimizations these guard against, so passing the
# value through unchanged matches the upstream observable behavior.
function compilerbarrier(setting, x)
    return x
end
