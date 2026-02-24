# =============================================================================
# BigInt/BigFloat utilities
# =============================================================================
# Based on Julia's base/gmp.jl and base/mpfr.jl
#
# Note: BigInt and BigFloat are primitive types in the VM.
# Core arithmetic operations (+, -, *, div, %, <, <=, >, >=, ==, !=) are
# handled by intrinsics. This file provides the `big` function for both
# value conversion and type conversion.

# =============================================================================
# big - Convert to maximum precision representation
# =============================================================================

# Type → Type conversions: big(::Type{T}) returns the "big" version of type T
# Based on Julia's base/gmp.jl and base/mpfr.jl

# Integer types → BigInt
function big(::Type{Int8})
    BigInt
end

function big(::Type{Int16})
    BigInt
end

function big(::Type{Int32})
    BigInt
end

function big(::Type{Int64})
    BigInt
end

function big(::Type{Int128})
    BigInt
end

function big(::Type{BigInt})
    BigInt
end

# Unsigned integer types → BigInt
function big(::Type{UInt8})
    BigInt
end

function big(::Type{UInt16})
    BigInt
end

function big(::Type{UInt32})
    BigInt
end

function big(::Type{UInt64})
    BigInt
end

function big(::Type{UInt128})
    BigInt
end

# Float types → BigFloat
function big(::Type{Float32})
    BigFloat
end

function big(::Type{Float64})
    BigFloat
end

function big(::Type{BigFloat})
    BigFloat
end

# Value → Value conversions

# big for integers - convert to BigInt
function big(x::Int64)
    return BigInt(x)
end

# big for BigInt - identity
function big(x::BigInt)
    return x
end

# big for floats - convert to BigFloat
function big(x::Float64)
    return BigFloat(x)
end

# big for BigFloat - identity
function big(x::BigFloat)
    return x
end

# =============================================================================
# BigInt predicates (Issue #416)
# Based on Julia's base/gmp.jl
# =============================================================================

# iszero for BigInt - check if value is zero
function iszero(x::BigInt)
    return x == big(0)
end

# isone for BigInt - check if value is one
function isone(x::BigInt)
    return x == big(1)
end

# sign for BigInt - returns -1, 0, or 1
function sign(x::BigInt)
    if x < big(0)
        return -1
    elseif x > big(0)
        return 1
    else
        return 0
    end
end

# =============================================================================
# BigInt/Integer comparison operators
# =============================================================================
# Note: BigInt/Int64 mixed comparisons (==, <, <=, >, >=) are handled directly
# by the VM's runtime dispatch in call_dynamic.rs. The intrinsics automatically
# promote Int64/I128 to BigInt for comparison operations.

# =============================================================================
# BigFloat Precision Control (Issue #345)
# Based on Julia's base/mpfr.jl
# =============================================================================

# precision(::Type{BigFloat}) - get the default precision for new BigFloat values
function precision(::Type{BigFloat})
    return _bigfloat_default_precision()
end

# precision(x::BigFloat) - get the precision of a specific BigFloat value
function precision(x::BigFloat)
    return _bigfloat_precision(x)
end

# setprecision(::Type{BigFloat}, precision::Integer) - set default precision
# Returns the new precision value
function setprecision(::Type{BigFloat}, prec::Int64)
    if prec < 1
        throw(DomainError(prec, "precision cannot be less than 1"))
    end
    _set_bigfloat_default_precision!(prec)
    return prec
end

# setprecision(precision::Integer) - set default precision (convenience)
function setprecision(prec::Int64)
    return setprecision(BigFloat, prec)
end

# setprecision(f::Function, ::Type{BigFloat}, precision::Integer) - run function with specific precision
# This temporarily changes the precision, runs f, then restores the old precision
function setprecision(f::Function, ::Type{BigFloat}, prec::Int64)
    old_prec = precision(BigFloat)
    setprecision(BigFloat, prec)
    try
        return f()
    finally
        setprecision(BigFloat, old_prec)
    end
end

# setprecision(f::Function, precision::Integer) - convenience form
function setprecision(f::Function, prec::Int64)
    return setprecision(f, BigFloat, prec)
end

# =============================================================================
# BigFloat Rounding Control (Issue #345)
# Based on Julia's base/mpfr.jl
# =============================================================================

# Internal: convert RoundingMode to mode integer
# 0=ToEven (RoundNearest), 1=ToZero, 2=Up, 3=Down, 4=FromZero
function _rounding_mode_to_int(mode::RoundingMode)
    if mode.mode == :Nearest
        return 0
    elseif mode.mode == :ToZero
        return 1
    elseif mode.mode == :Up
        return 2
    elseif mode.mode == :Down
        return 3
    elseif mode.mode == :FromZero
        return 4
    else
        return 0  # Default to RoundNearest
    end
end

# Internal: convert mode integer to RoundingMode
# Note: We construct RoundingMode directly instead of using const values
# (RoundNearest, etc.) because global const structs with arguments are not
# accessible from function bodies in SubsetJuliaVM.
function _int_to_rounding_mode(mode::Int64)
    if mode == 0
        return RoundingMode(:Nearest)  # RoundNearest
    elseif mode == 1
        return RoundingMode(:ToZero)   # RoundToZero
    elseif mode == 2
        return RoundingMode(:Up)       # RoundUp
    elseif mode == 3
        return RoundingMode(:Down)     # RoundDown
    elseif mode == 4
        return RoundingMode(:FromZero) # RoundFromZero
    else
        return RoundingMode(:Nearest)  # Default (RoundNearest)
    end
end

# rounding(::Type{BigFloat}) - get the current rounding mode for BigFloat
function rounding(::Type{BigFloat})
    mode = _bigfloat_rounding()
    return _int_to_rounding_mode(mode)
end

# setrounding(::Type{BigFloat}, mode::RoundingMode) - set rounding mode
function setrounding(::Type{BigFloat}, mode::RoundingMode)
    mode_int = _rounding_mode_to_int(mode)
    _set_bigfloat_rounding!(mode_int)
    return mode
end

# setrounding(f::Function, ::Type{BigFloat}, mode::RoundingMode) - run function with specific rounding mode
function setrounding(f::Function, ::Type{BigFloat}, mode::RoundingMode)
    old_mode = rounding(BigFloat)
    setrounding(BigFloat, mode)
    try
        return f()
    finally
        setrounding(BigFloat, old_mode)
    end
end
