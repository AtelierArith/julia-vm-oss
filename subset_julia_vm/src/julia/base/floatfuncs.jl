# =============================================================================
# Float Functions - Floating-point utilities
# =============================================================================
# Based on Julia's base/floatfuncs.jl
#
# IMPORTANT: This module only contains functions that exist in Julia Base.
# Removed functions (not in Julia Base):
#   - trunc_int (use trunc builtin)
#   - frexp_approx (use frexp builtin)
#   - ulp_approx (use eps)
#   - round_digits (use round(x, digits=n))
#
# NOTE: exponent / significand / frexp / issubnormal / nextfloat / prevfloat are
# now pure Julia in base/float.jl (Issue #6740), built on `reinterpret` + the
# per-type IEEE bit-field helpers there. They are no longer Rust builtins.

# =============================================================================
# Rounding: floor / ceil / trunc / round (Issue #6742)
# =============================================================================
# Pure-Julia rounding over the CPU intrinsics floor_llvm / ceil_llvm /
# trunc_llvm / rint_llvm (base/boot.jl). These dispatch-first ahead of the legacy
# Rust round builtins (which remain only as a no-method fallback). `round` uses
# round-to-nearest-ties-to-even (the default `RoundNearest`), matching upstream
# (round(2.5) == 2.0, round(0.5) == 0.0) — the old Rust dynamic-dispatch path
# wrongly used round-half-away-from-zero.
#
# The plain forms use `where {T<:AbstractFloat}` so the concrete float type is
# preserved (round(3.0f0) isa Float32). The digits/sigdigits/base keyword forms
# are folded into the same methods (keyword args default to `nothing`).

# --- float forms (preserve the float type) with digits/sigdigits/base keywords ---
# Each method folds the plain form (no keywords → the CPU intrinsic) and the
# digits/sigdigits scaling into one definition (a positional method and a keyword
# method would share the same signature and conflict). The scaling reproduces the
# `x*factor`/`factor` algorithm of the former Rust round-digits builtins. `_scale`
# returns the (factor) for a given digits/sigdigits request.
function _round_scale(x, digits, sigdigits, base)
    if digits !== nothing
        return float(base) ^ digits
    end
    # sigdigits: scale so `sigdigits` significant digits survive (base-`base`).
    d = floor(Int, log(float(base), abs(x))) + 1
    return float(base) ^ (sigdigits - d)
end

function floor(x::T; digits=nothing, sigdigits=nothing, base=10) where {T<:AbstractFloat}
    if digits === nothing && sigdigits === nothing
        return floor_llvm(x)
    end
    (x == 0 || (sigdigits !== nothing && sigdigits <= 0)) && return zero(x)
    factor = _round_scale(x, digits, sigdigits, base)
    return floor_llvm(x * factor) / factor
end

function ceil(x::T; digits=nothing, sigdigits=nothing, base=10) where {T<:AbstractFloat}
    if digits === nothing && sigdigits === nothing
        return ceil_llvm(x)
    end
    (x == 0 || (sigdigits !== nothing && sigdigits <= 0)) && return zero(x)
    factor = _round_scale(x, digits, sigdigits, base)
    return ceil_llvm(x * factor) / factor
end

function trunc(x::T; digits=nothing, sigdigits=nothing, base=10) where {T<:AbstractFloat}
    if digits === nothing && sigdigits === nothing
        return trunc_llvm(x)
    end
    (x == 0 || (sigdigits !== nothing && sigdigits <= 0)) && return zero(x)
    factor = _round_scale(x, digits, sigdigits, base)
    return trunc_llvm(x * factor) / factor
end

function round(x::T; digits=nothing, sigdigits=nothing, base=10) where {T<:AbstractFloat}
    if digits === nothing && sigdigits === nothing
        return rint_llvm(x)
    end
    (x == 0 || (sigdigits !== nothing && sigdigits <= 0)) && return zero(x)
    factor = _round_scale(x, digits, sigdigits, base)
    return rint_llvm(x * factor) / factor
end

# --- integer identity (floor(5) === 5, not 5.0) ---
floor(x::Integer) = x
ceil(x::Integer) = x
trunc(x::Integer) = x
round(x::Integer) = x

# --- typed forms: floor(Int, x) etc. round, then convert to the requested type
#     (round(Int8, 3.5) === Int8(4), preserving the requested integer type) ---
floor(::Type{T}, x) where {T<:Integer} = T(floor(x))
ceil(::Type{T}, x) where {T<:Integer} = T(ceil(x))
trunc(::Type{T}, x) where {T<:Integer} = T(trunc(x))
round(::Type{T}, x) where {T<:Integer} = T(round(x))

# isinteger: check if value is an integer
function isinteger(x)
    return x == floor(x)
end

# cld: ceiling division - returns integer type for integers, float for floats
# Julia's cld returns the same type as input for integers
function cld(x::Int64, y::Int64)
    # ceil() returns Float64, convert back to Int64
    return Int64(ceil(x / y))
end

function cld(x::Float64, y::Float64)
    return ceil(x / y)
end

function cld(x, y)
    return ceil(x / y)
end

# modf: return (fractional part, integer part)
# trunc(x) = sign(x) * floor(abs(x)) to preserve sign for negative numbers
function modf(x)
    ax = abs(x)
    i_abs = floor(ax)
    i = x >= 0 ? i_abs : -i_abs
    f = x - i
    return (f, i)
end

# ldexp: load exponent (x * 2^n)
function ldexp(x, n)
    return x * (2.0 ^ n)
end

# =============================================================================
# Special value predicates
# =============================================================================

# isnan: check if value is NaN (Not a Number)
# NaN is the only value where x != x
function isnan(x)
    return x != x
end

# isinf: check if value is infinite (+Inf or -Inf)
# Infinity minus itself is NaN, finite values give 0
function isinf(x)
    return !isnan(x) && isnan(x - x)
end

# isfinite: check if value is finite (not NaN and not Inf)
function isfinite(x)
    return !isnan(x) && !isinf(x)
end

# =============================================================================
# Floating-point properties
# =============================================================================

# eps: machine epsilon - the difference between 1.0 and the next larger Float64
# eps() returns eps(Float64)
# eps(::Type{Float64}) returns the machine epsilon for Float64
# eps(x::Float64) returns the spacing at x (ulp - unit in last place)
function eps()
    return 2.220446049250313e-16
end

function eps(::Type{Float64})
    return 2.220446049250313e-16
end

function eps(x::Float64)
    if !isfinite(x)
        return 0.0 / 0.0  # NaN
    end
    ax = abs(x)
    if ax >= 2.2250738585072014e-308  # floatmin(Float64)
        # Normal range: scale eps by power of 2
        # Find the exponent and scale
        e = 0
        temp = ax
        while temp >= 2.0
            temp = temp / 2.0
            e = e + 1
        end
        while temp < 1.0 && temp > 0.0
            temp = temp * 2.0
            e = e - 1
        end
        return ldexp(2.220446049250313e-16, e)
    else
        # Subnormal range: return smallest positive Float64
        return 5.0e-324
    end
end

# floatmin: smallest positive normalized floating-point number
function floatmin()
    return 2.2250738585072014e-308
end

function floatmin(::Type{Float64})
    return 2.2250738585072014e-308
end

function floatmin(x::Float64)
    return 2.2250738585072014e-308
end

# Float32 / Float16 smallest positive normalized values (Issue #5688).
function floatmin(::Type{Float32})
    return Float32(1.1754943508222875e-38)
end

function floatmin(::Type{Float16})
    return Float16(6.103515625e-5)
end

# floatmax: largest finite floating-point number
function floatmax()
    return 1.7976931348623157e308
end

function floatmax(::Type{Float64})
    return 1.7976931348623157e308
end

function floatmax(x::Float64)
    return 1.7976931348623157e308
end

# Float32 / Float16 largest finite values (Issue #5688).
function floatmax(::Type{Float32})
    return Float32(3.4028234663852886e38)
end

function floatmax(::Type{Float16})
    return Float16(65504.0)
end

# =============================================================================
# Integer type properties
# =============================================================================

# typemin: minimum value for an integer type
function typemin(::Type{Int64})
    # -9223372036854775808 cannot be parsed directly (overflow)
    # Use -typemax(Int64) - 1 instead
    return -9223372036854775807 - 1
end

function typemin(::Type{Int32})
    return Int32(-2147483648)
end

function typemin(::Type{Int16})
    return Int16(-32768)
end

function typemin(::Type{Int8})
    return Int8(-128)
end

# typemax: maximum value for an integer type
function typemax(::Type{Int64})
    return 9223372036854775807
end

function typemax(::Type{Int32})
    return Int32(2147483647)
end

function typemax(::Type{Int16})
    return Int16(32767)
end

function typemax(::Type{Int8})
    return Int8(127)
end

# typemin/typemax for floating-point types (Issue #2094)
# In Julia, typemin(Float64) = -Inf, typemax(Float64) = Inf
function typemin(::Type{Float64})
    return -Inf
end

function typemax(::Type{Float64})
    return Inf
end

function typemin(::Type{Float32})
    return Float32(-Inf)
end

function typemax(::Type{Float32})
    return Float32(Inf)
end

function typemin(::Type{Float16})
    return Float16(-Inf)
end

function typemax(::Type{Float16})
    return Float16(Inf)
end

# Note: UInt typemax/typemin are defined in int.jl (Issue #3143 — UInt8/UInt16/UInt32 added)

# =============================================================================
# Timing functions
# =============================================================================

# time: return current time in seconds (since epoch)
# Note: time_ns() is a VM builtin
function time()
    return time_ns() / 1_000_000_000.0
end

# =============================================================================
# unsafe_trunc: Unsafe truncation (no error checking)
# =============================================================================

# unsafe_trunc: truncate without checking for overflow/inexactness
# This is unsafe because it doesn't check if the value is representable
function unsafe_trunc(::Type{Int64}, x::Float64)
    # Use trunc to get integer part, then convert to Int64
    # This may overflow or produce incorrect results for NaN/Inf
    return Int64(trunc(x))
end

function unsafe_trunc(::Type{Int32}, x::Float64)
    return Int32(trunc(x))
end

function unsafe_trunc(::Type{Int16}, x::Float64)
    return Int16(trunc(x))
end

function unsafe_trunc(::Type{Int8}, x::Float64)
    return Int8(trunc(x))
end

# Note: Generic unsafe_trunc(::Type{T}, x::Float64) where T<:Integer is not supported
# in SubsetJuliaVM due to where clause limitations. Use explicit type methods above.

# =============================================================================
# maxintfloat: Largest consecutive integer representable as Float
# =============================================================================
# Based on Julia's base/floatfuncs.jl:32-45

# maxintfloat(): returns maxintfloat(Float64) by default
function maxintfloat()
    return 9007199254740992.0
end

# maxintfloat(::Type{Float64}): 2^53 (largest exact integer in Float64)
function maxintfloat(::Type{Float64})
    return 9007199254740992.0
end

# maxintfloat(x::Float64): for value, return type's maxintfloat
function maxintfloat(x::Float64)
    return 9007199254740992.0
end

# =============================================================================
# precision: Number of significant bits in Float64
# =============================================================================
# Based on Julia's base/float.jl:798-807

# precision(::Type{Float64}): 53 bits (52 mantissa bits + 1 implicit)
function precision(::Type{Float64})
    return 53
end

# precision(x::Float64): for value, return type's precision
function precision(x::Float64)
    return 53
end
