# =============================================================================
# Missing - Missing value support
# =============================================================================
# Based on Julia's base/missing.jl
# upstream: julia/base/missing.jl @ 15346901f0039751c5488744f1f62de7d87510a8 (swept 2026-06-28)
#
# The Missing type represents missing data in statistical and data analysis contexts.
# Unlike `nothing` (absence of value), `missing` represents unknown or unavailable data.

# ismissing: check if value is missing
# INTENTIONAL_NOOP (Issue #4703): upstream `ismissing(x) = x === missing`
# (julia/base/essentials.jl:1491) is exactly this identity comparison, so
# the trivial `return x === missing` body is correct, not a stub.
function ismissing(x)
    return x === missing
end

# nonmissingtype(T) removes Missing from a Union type. The public method is
# Julia-owned; the underscored boundary keeps the existing runtime type lattice
# operation while Ref/Compose/deepcopy migrate in the same Issue #8779 slice.
nonmissingtype(T) = _nonmissingtype(T)

# coalesce: return the first non-missing value
# coalesce(x, y) returns x if x is not missing, otherwise y
function coalesce(x, y)
    if ismissing(x)
        return y
    else
        return x
    end
end

# coalesce with 3 arguments
function coalesce(x, y, z)
    if ismissing(x)
        if ismissing(y)
            return z
        else
            return y
        end
    else
        return x
    end
end

# coalesce with 4 arguments
function coalesce(x, y, z, w)
    if ismissing(x)
        if ismissing(y)
            if ismissing(z)
                return w
            else
                return z
            end
        else
            return y
        end
    else
        return x
    end
end

# =============================================================================
# isequal for Missing
# =============================================================================
# isequal returns Bool (not missing), which is important for sorting and hashing.
# Two missing values are considered equal (isequal(missing, missing) = true),
# but missing is not equal to any other value.

# isequal(::Missing, ::Missing) = true
function isequal(x::Missing, y::Missing)
    return true
end

# isequal(::Missing, ::Any) = false
function isequal(x::Missing, y)
    return false
end

# isequal(::Any, ::Missing) = false
function isequal(x, y::Missing)
    return false
end

# =============================================================================
# isless for Missing
# =============================================================================
# isless defines a total order for sorting. Missing values sort to the end
# (are considered greater than all other values).
# isless returns Bool (not missing).

# isless(::Missing, ::Missing) = false (missing is not less than itself)
function isless(x::Missing, y::Missing)
    return false
end

# isless(::Missing, ::Any) = false (missing is not less than anything)
function isless(x::Missing, y)
    return false
end

# isless(::Any, ::Missing) = true (everything is less than missing)
function isless(x, y::Missing)
    return true
end

# =============================================================================
# isequal for Missing (Issue #2718)
# =============================================================================
function isequal(a::Missing, b::Missing)
    return true
end
function isequal(a::Missing, b)
    return false
end
function isequal(a, b::Missing)
    return false
end

# Issues #10612/#10693: `div(x, y)` in sjulia currently performs its own
# promote/trunc path instead of upstream's `div(x, y, RoundToZero)` bridge, so
# keep Missing propagation explicit for the 2-arg entry as well as the 3-arg
# upstream shape.
div(::Missing, ::Missing) = missing
div(::Missing, ::Number) = missing
div(::Number, ::Missing) = missing

# Nothing specialization for isequal (Issue #2718)
function isequal(a::Nothing, b::Nothing)
    return true
end

# Cross-type numeric specializations for isequal (Issue #2718).
# Use the value-based `==` (Issue #8187: `Float64(a)` would round the integer for
# |a| > 2^53, making isequal(2^53+1, 2.0^53) wrongly true) and add isequal's
# signed-zero distinction: an integer is always +0, so isequal(0, -0.0) is false
# even though 0 == -0.0 is true.
function isequal(a::Int64, b::Float64)
    return (a == b) && !(b == 0.0 && signbit(b))
end
function isequal(a::Float64, b::Int64)
    return (a == b) && !(a == 0.0 && signbit(a))
end

# Array specialization: element-wise isequal with shape check (Issue #2718)
function isequal(A::Array, B::Array)
    if length(A) != length(B)
        return false
    end
    if ndims(A) != ndims(B)
        return false
    end
    for d in 1:ndims(A)
        if size(A, d) != size(B, d)
            return false
        end
    end
    for i in 1:length(A)
        if isequal(A[i], B[i]) == false
            return false
        end
    end
    return true
end

# Tuple specialization: element-wise isequal (Issue #2718)
function isequal(t1::Tuple, t2::Tuple)
    if length(t1) != length(t2)
        return false
    end
    for i in 1:length(t1)
        if isequal(t1[i], t2[i]) == false
            return false
        end
    end
    return true
end

# Expr specialization: structural comparison via `==` (Issue #2718, #9264).
# Upstream has no dedicated `isequal(::Expr, ::Expr)`; it falls back to the
# generic `isequal(x, y) = x == y`, which routes to the field-structural
# `==(x::Expr, y::Expr) = x.head === y.head && isequal(x.args, y.args)`
# (base/expr.jl). Mirror that: use `==` (structural), NOT `===`. Since `===`
# is now object identity for the mutable `Expr` (Issue #9264), a `=== a`
# definition here would make nested `isequal(x.args, y.args)` — reached for an
# `Expr` element of another Expr's args — compare by identity and break
# structural `==`/`isequal` on independently-built Exprs (regressing #9183).
function isequal(a::Expr, b::Expr)
    return a == b
end

# =============================================================================
# isunordered for Missing (Issue #2715)
# =============================================================================
# Missing values are unordered — comparisons with missing are undefined.
# Based on Julia's base/operators.jl:293
isunordered(x::Missing) = true

# =============================================================================
# Note on ispositive, isnegative, isapprox, min, and max for Missing
# =============================================================================
# In Julia, these functions should return `missing` for Missing values:
#   ispositive(::Missing) = missing
#   isnegative(::Missing) = missing
#   isapprox(::Missing, ::Any) = missing
#   min(::Missing, ::Any) = missing
#   max(::Missing, ::Any) = missing
#
# However, the current method dispatch system doesn't properly select
# type-specific methods (x::Missing) over generic methods (x).
# This requires additional work on the method dispatch system (Issue #719).
#
# For now, comparison operators (==, <, >, etc.) with literal `missing` values
# are handled at compile-time in binary.rs.

# =============================================================================
# Three-valued logic (Issue #10692)
# =============================================================================
# Upstream base/missing.jl:158-172: bitwise &, |, xor over Bool/Integer and
# Missing follow Kleene logic — `false & missing` is `false`, `true | missing`
# is `true`, everything else involving missing is missing. Registered here in
# missing.jl, which loads AFTER int.jl, so the Int64 methods stay the
# first-registered runtime fallback for mixed-type bitwise calls (the
# dispatch-order contract documented in base/bool.jl, Issue #8197).
Base.:(&)(::Missing, ::Missing) = missing
Base.:(&)(a::Missing, b::Bool) = ifelse(b, missing, false)
Base.:(&)(b::Bool, a::Missing) = ifelse(b, missing, false)
Base.:(&)(::Missing, ::Integer) = missing
Base.:(&)(::Integer, ::Missing) = missing
Base.:(|)(::Missing, ::Missing) = missing
Base.:(|)(a::Missing, b::Bool) = ifelse(b, true, missing)
Base.:(|)(b::Bool, a::Missing) = ifelse(b, true, missing)
Base.:(|)(::Missing, ::Integer) = missing
Base.:(|)(::Integer, ::Missing) = missing
xor(::Missing, ::Missing) = missing
xor(a::Missing, b::Bool) = missing
xor(b::Bool, a::Missing) = missing
xor(::Missing, ::Integer) = missing
xor(::Integer, ::Missing) = missing
Base.:(⊻)(::Missing, ::Missing) = missing
Base.:(⊻)(a::Missing, b::Bool) = missing
Base.:(⊻)(b::Bool, a::Missing) = missing
Base.:(⊻)(::Missing, ::Integer) = missing
Base.:(⊻)(::Integer, ::Missing) = missing
