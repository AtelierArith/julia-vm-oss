# =============================================================================
# Bool - Boolean operations
# =============================================================================
# Based on Julia's base/bool.jl
#
# IMPORTANT: This module only contains functions that exist in Julia Base.
# Removed functions (not in Julia Base):
#   - implies (not in Julia)

# isnothing: check if value is nothing
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

# Power with integer base
# Based on Julia's base/bool.jl:22
^(x::Integer, y::Bool) = ifelse(y, x, one(x))

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
