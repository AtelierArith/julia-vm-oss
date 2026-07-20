# =============================================================================
# Bool - Boolean operations
# =============================================================================
# Based on Julia's base/bool.jl
# upstream: julia/base/bool.jl @ 15346901f0039751c5488744f1f62de7d87510a8 (swept 2026-07-02)
#
# IMPORTANT: This module only contains functions that exist in Julia Base.
# Removed functions (not in Julia Base):
#   - implies (not in Julia)

# Bool constructor
# Based on Julia's base/bool.jl:
#   Bool(x::Real) = x==0 ? false : x==1 ? true : throw(InexactError(:Bool, Bool, x))
function Bool(x::Real)
    return x == 0 ? false : x == 1 ? true : throw(InexactError(:Bool, Bool, x))
end

# isnothing: check if value is nothing
# INTENTIONAL_NOOP (Issue #4703): upstream `isnothing(x) = x === nothing`
# (julia/base/some.jl:67) is exactly this identity comparison, so the
# trivial `return x === nothing` body is correct, not an unfinished stub.
function isnothing(x)
    return x === nothing
end

# xor: exclusive or
function xor(x, y)
    if x
        return !y
    else
        return y
    end
end

# xor: bitwise exclusive or (Int64) - Issue #2042
function xor(x::Int64, y::Int64)
    return xor_int(x, y)
end

# ~: bitwise NOT for Bool (Issue #7305)
# Based on Julia's base/bool.jl:13 -- (~)(x::Bool) = !x
function Base.:(~)(x::Bool)
    return !x
end

# NOTE (Issue #8197): the Bool bitwise operators `&` / `|` / `xor` / `⊻`
# (upstream base/bool.jl:14-15,49) are intentionally defined in base/int.jl,
# AFTER the `Int64` methods, rather than here. Reason: a mixed-type bitwise call
# with no exact same-type method (e.g. `0x05 & 5`, or any `&`/`|` inside a
# generic function where the operands are statically `Any`) is dispatched at
# runtime via `CallTypedDispatch`, whose no-match fallback is the FIRST method
# registered for that operator. The `Int64` method (`and_int`/`or_int`, which
# widens both operands to `Int64`) is a type-safe fallback that matches upstream
# (`0x05 & 5 === 5`); a `Bool` method registered first would instead make a
# `Bool`-typed result slot receive a widened `Int64` at runtime → `LoadSlotBool`.
# Defining the Bool methods after the Int64 methods keeps `Int64` as the
# fallback. See base/int.jl for the actual definitions.

# nand: not and
function nand(x, y)
    if x && y
        return false
    else
        return true
    end
end

# nor: not or
function nor(x, y)
    if x || y
        return false
    else
        return true
    end
end

# =============================================================================
# Sign-related functions for Bool (based on Julia's base/bool.jl:153-156)
# =============================================================================

# signbit for Bool - always false
# Based on Julia's base/bool.jl:153
function signbit(x::Bool)
    return false
end

# sign for Bool - returns itself
# Based on Julia's base/bool.jl:154
function sign(x::Bool)
    return x
end

# abs for Bool - returns itself
# Based on Julia's base/bool.jl:155
function abs(x::Bool)
    return x
end

# abs2 for Bool - returns itself
# Based on Julia's base/bool.jl:156
function abs2(x::Bool)
    return x
end

# =============================================================================
# Type bounds for Bool (based on Julia's base/bool.jl:8-9)
# =============================================================================

# typemin for Bool - false
# Based on Julia's base/bool.jl:8
function typemin(::Type{Bool})
    return false
end

# typemax for Bool - true
# Based on Julia's base/bool.jl:9
function typemax(::Type{Bool})
    return true
end

# =============================================================================
# Number predicates for Bool (based on Julia's base/bool.jl:157-158)
# =============================================================================

# iszero for Bool - true only if false
# Based on Julia's base/bool.jl:157
function iszero(x::Bool)
    return !x
end

# isone for Bool - true only if true
# Based on Julia's base/bool.jl:158
function isone(x::Bool)
    return x
end

# NOTE: ispositive(x::Bool) = x is defined in Julia 1.13+ (base/bool.jl:159)
# Not implemented here yet for Julia 1.12 compatibility.

# =============================================================================
# Arithmetic operations for Bool (based on Julia's base/bool.jl:14-25)
# =============================================================================
# Bool arithmetic is done by converting to Int

# Unary operators
# Based on Julia's base/bool.jl:14-15
Base.:(+)(x::Bool) = Int(x)
Base.:(-)(x::Bool) = -Int(x)

# Binary operators
# Based on Julia's base/bool.jl:17-21
Base.:(+)(x::Bool, y::Bool) = Int(x) + Int(y)
Base.:(-)(x::Bool, y::Bool) = Int(x) - Int(y)
# Note: Using && instead of & for Bool to avoid unsupported bitwise operators
*(x::Bool, y::Bool) = x && y
# Note: Using || instead of | for Bool to avoid unsupported bitwise operators
^(x::Bool, y::Bool) = x || !y

# Preserve Bool division result type, matching Julia's base/bool.jl.
function div(x::Bool, y::Bool)
    return y ? x : throw(DivideError())
end

# rem/mod/fld/cld on Bool×Bool stay Bool (Issue #9337; upstream base/bool.jl
# defines rem/mod, and fld/cld reduce to div for the Bool same-type case).
function rem(x::Bool, y::Bool)
    return y ? false : throw(DivideError())
end

function mod(x::Bool, y::Bool)
    return rem(x, y)
end

function fld(x::Bool, y::Bool)
    return div(x, y)
end

function cld(x::Bool, y::Bool)
    return div(x, y)
end

# Power with integer base
# Based on Julia's base/bool.jl:22
^(x::Integer, y::Bool) = ifelse(y, x, one(x))

# Bool "strong zero" multiply (Issue #9343)
# Based on Julia's base/bool.jl:
#   *(x::Bool, y::T) where {T<:AbstractFloat} = ifelse(x, y, copysign(zero(y), y))
#   *(y::AbstractFloat, x::Bool) = x * y
# `false * y` is a strong zero (`copysign(zero(y), y)`), stronger than IEEE NaN
# propagation, so `false * Inf == 0.0` and `false * -Inf == -0.0` (not NaN).
# For the primitive float widths (Float16/Float32/Float64) this is intercepted in
# the VM before Bool→Int normalization; this pure-Julia method covers the
# remaining AbstractFloat subtypes (e.g. BigFloat) via dispatch.
*(x::Bool, y::T) where {T<:AbstractFloat} = ifelse(x, y, copysign(zero(y), y))
*(y::AbstractFloat, x::Bool) = x * y

# =============================================================================
# Comparison operations for Bool (using intrinsics)
# =============================================================================
# These prevent the Number fallback from being used for Bool comparisons

function Base.:(==)(x::Bool, y::Bool)
    eq_int(Int64(x), Int64(y))
end

function Base.:(!=)(x::Bool, y::Bool)
    ne_int(Int64(x), Int64(y))
end

function Base.:(<)(x::Bool, y::Bool)
    # false < true in Julia
    slt_int(Int64(x), Int64(y))
end

function Base.:(<=)(x::Bool, y::Bool)
    sle_int(Int64(x), Int64(y))
end

function Base.:(>)(x::Bool, y::Bool)
    sgt_int(Int64(x), Int64(y))
end

function Base.:(>=)(x::Bool, y::Bool)
    sge_int(Int64(x), Int64(y))
end
