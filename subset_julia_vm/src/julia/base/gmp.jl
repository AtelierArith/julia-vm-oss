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

# big for integers - convert to BigInt. Upstream (base/gmp.jl) defines a single
# `big(n::Integer) = convert(BigInt, n)` rather than a per-width method; keep the
# abstract signature so a `BigInt` argument can NEVER be dispatched into a
# concrete `big(x::Int64)` (machine-`I64` slot) method — a `#5966`-class
# dispatch-order flake would otherwise land a `BigInt` in that `I64` slot and
# panic with `LoadSlotI64: expected numeric in x, got BigInt(...)` (Issue #9724).
# `BigInt(x)` is already the identity on a `BigInt`, so this also subsumes the
# old `big(x::BigInt)` method.
function big(x::Integer)
    return BigInt(x)
end

# big for BigInt - identity (kept as the most-specific method; also the safe
# target if a `BigInt` reaches `big` via the abstract method above)
function big(x::BigInt)
    return x
end

# big for floats - convert to BigFloat. Abstract signature for the same reason as
# the integer method above (`BigFloat(x)` is the identity on a `BigFloat`).
function big(x::AbstractFloat)
    return BigFloat(x)
end

# big for BigFloat - identity
function big(x::BigFloat)
    return x
end

# `string(x)` is a public Pure Julia method after Issue #8780, but BigFloat
# still needs the VM's Julia-compatible MPFR/astro-float formatter.
string(x::BigFloat) = _string(x)

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
        return big(-1)
    elseif x > big(0)
        return big(1)
    else
        return big(0)
    end
end

# inv for BigInt - multiplicative inverse as a BigFloat (Issue #7309)
# Upstream: inv(x::Integer) = float(one(x)) / float(x) (julia/base/int.jl:94);
# float(::BigInt) is a BigFloat, so inv(big(2)) == 0.5 :: BigFloat rather than
# the integer division (1 ÷ 2 == 0) that the generic inv(x::Number) = one(x)/x
# fallback produced for BigInt.
function inv(x::BigInt)
    return inv(BigFloat(x))
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

# =============================================================================
# nextfloat / prevfloat for BigFloat (Issue #9280)
# Based on Julia's base/mpfr.jl (mpfr_nextabove / mpfr_nextbelow semantics).
# =============================================================================
#
# The generic `nextfloat(x::T) where {T<:AbstractFloat}` in base/float.jl steps
# the bit pattern via `reinterpret(Int64, x)`, which cannot apply to BigFloat
# (its payload is not 8 bytes). These BigFloat-specific methods advance by one
# ULP at the value's own precision via `_bigfloat_nextfloat`, which performs the
# step on the astro_float backend (sjulia's BigFloat is not MPFR-backed),
# correctly handling 0, ±Inf, NaN, negatives, and power-of-two boundaries.
nextfloat(x::BigFloat) = _bigfloat_nextfloat(x, true)
prevfloat(x::BigFloat) = _bigfloat_nextfloat(x, false)

# nextfloat(x, n): step n ULPs (upstream loops mpfr_nextabove; n < 0 steps down,
# n == 0 returns x unchanged).
function nextfloat(x::BigFloat, n::Integer)
    n == 0 && return x
    up = n > 0
    r = x
    for _ in 1:abs(n)
        r = _bigfloat_nextfloat(r, up)
    end
    return r
end

# prevfloat(x, n): step n ULPs downward (the negative-direction of nextfloat).
prevfloat(x::BigFloat, n::Integer) = nextfloat(x, -n)

# floatmin / floatmax for BigFloat (Issue #9290), mirroring upstream
# julia/base/mpfr.jl: the smallest/largest positive finite BigFloat at the
# current precision. NOTE: the *values* differ from upstream MPFR because
# sjulia's astro_float backend has an i32-class exponent range (~10^±6.46e8)
# vs MPFR's emax ≈ 2^62 (~10^±1.39e18) — a documented backend divergence
# (Issue #9290, same family as Issue #8885). The invariants
# floatmax(BigFloat) == prevfloat(BigFloat(Inf)) and
# floatmin(BigFloat) == nextfloat(zero(BigFloat)) hold as in upstream.
floatmin(::Type{BigFloat}) = nextfloat(zero(BigFloat))
floatmax(::Type{BigFloat}) = prevfloat(BigFloat(Inf))

# =============================================================================
# exponent / significand / frexp for BigFloat (Issue #9286)
# Based on Julia's base/mpfr.jl (mpfr_get_exp / mpfr_frexp semantics).
# =============================================================================
#
# The generic `where {T<:AbstractFloat}` definitions in base/float.jl decode the
# Float64 bit pattern via `reinterpret(UInt64, x)`, which cannot apply to a
# BigFloat (its payload is not 8 bytes → "reinterpret size mismatch"). These
# BigFloat-specific methods operate on the astro_float exponent instead
# (sjulia's BigFloat is not MPFR-backed): for a finite nonzero `x`,
# `_bigfloat_get_exp(x)` returns the exponent `E` with `x = m·2^E`, `m ∈ [0.5, 1)`
# (MPFR's mpfr_get_exp / mpfr_frexp convention), and `_bigfloat_scale2(x, n)`
# multiplies by `2^n` exactly.

# exponent(x): unbiased base-2 exponent. MPFR's `mpfr_get_exp - 1` maps the
# [0.5, 1) mantissa convention onto Base's [1, 2) convention, so
# `exponent(BigFloat("1.5")) == 0`. Zero and non-finite values throw DomainError.
function exponent(x::BigFloat)
    if iszero(x) || !isfinite(x)
        throw(DomainError(x, "Cannot be ±0.0, NaN or Inf."))
    end
    return _bigfloat_get_exp(x) - 1
end

# frexp(x): (m, E) with `m ∈ [0.5, 1)` (keeping x's sign) and `x = m·2^E`, so
# `frexp(BigFloat("1.5")) == (0.75, 1)`. Zero and non-finite values return
# `(x, 0)`, matching mpfr_frexp / Base.frexp.
function frexp(x::BigFloat)
    if iszero(x) || !isfinite(x)
        return (x, 0)
    end
    e = _bigfloat_get_exp(x)
    return (_bigfloat_scale2(x, -e), e)
end

# significand(x): x normalized to [1, 2) keeping its sign (Base's convention),
# so `significand(BigFloat("1.5")) == 1.5`. This is the frexp mantissa doubled,
# i.e. `x·2^(1-E)`. Zero and non-finite values return x unchanged.
function significand(x::BigFloat)
    if iszero(x) || !isfinite(x)
        return x
    end
    e = _bigfloat_get_exp(x)
    return _bigfloat_scale2(x, 1 - e)
end

# =============================================================================
# signbit for BigFloat (Issue #9450)
# Based on Julia's base/mpfr.jl: `signbit(x::BigFloat) = signbit(x.sign)` —
# the sign field is read directly so a negative zero is observable. The generic
# `signbit(x) = x < 0` (base/number.jl) cannot see `-zero(BigFloat)`, which
# mis-signed abs/copysign/flipsign/mod of BigFloat zeros. `_bigfloat_signbit`
# reads the astro_float sign field (false for NaN, matching MPFR/Julia).
# =============================================================================
signbit(x::BigFloat) = _bigfloat_signbit(x)

# BigFloat → BigInt conversion (Issue #9424)
# Based on Julia's base/mpfr.jl (`BigInt(::BigFloat)`,
# `round(::Type{BigInt}, ::BigFloat)`).
# =============================================================================
#
# `BigInt(x::BigFloat)` itself is the exact Rust-builtin conversion: it reads
# astro_float's raw mantissa/exponent and throws `InexactError` unless `x` is a
# finite exact integer, mirroring upstream mpfr.jl. `round(BigInt, x)` first
# rounds to the nearest integer at x's own precision (ties to even — the
# default rounding, matching MPFR's RoundNearest), then converts exactly.
function round(::Type{BigInt}, x::BigFloat)
    isfinite(x) || throw(InexactError(:round, BigInt, x))
    return BigInt(round(x))
end
