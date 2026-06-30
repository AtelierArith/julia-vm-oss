# =============================================================================
# Math functions - Pure Julia implementations
# =============================================================================

# sign: return the sign of a number (-1, 0, or 1)
function sign(x)
    if x > 0
        return 1
    elseif x < 0
        return -1
    else
        return 0
    end
end

function sign(x::Unsigned)
    if x > oftype(x, 0)
        return oftype(x, 1)
    else
        return x
    end
end

function sign(x::Real)
    if x > oftype(x, 0)
        return oftype(x, 1)
    elseif x < oftype(x, 0)
        return oftype(x, -1)
    else
        return x
    end
end

# sign for BigInt: compare with BigInt(0) to avoid mixed-type comparison
function sign(x::BigInt)
    zero_bi = BigInt(0)
    if x > zero_bi
        return BigInt(1)
    elseif x < zero_bi
        return BigInt(-1)
    else
        return zero_bi
    end
end

# clamp: constrain a value between lo and hi
function clamp(x, lo, hi)
    if x < lo
        return lo
    elseif x > hi
        return hi
    else
        return x
    end
end

# clamp(x, r): constrain an Integer to a unit range's bounds. Matches upstream
# `clamp(x::Integer, r::AbstractUnitRange)`; a Float `x` or a StepRange `r` has no
# such method (a MethodError), like upstream.
function clamp(x::Integer, r::AbstractUnitRange)
    return clamp(x, first(r), last(r))
end

# mod: modulo operation (result has same sign as divisor)
function mod(x, y)
    r = x % y
    if r != 0 && (r < 0) != (y < 0)
        return r + y
    else
        return r
    end
end

# div: integer division, rounding toward zero (RoundToZero).
# Mirrors upstream `div(a, b) = div(a, b, RoundToZero)` — NOT floor division
# (that is `fld`). Differs from `fld`/`cld` only for operands of opposite sign,
# e.g. div(-7.0, 3.0) == -2.0 while fld(-7.0, 3.0) == -3.0 (Issue #6891).
function div(x, y)
    return trunc(x / y)
end

# hypot: hypotenuse length sqrt(x^2 + y^2)
function hypot(x, y)
    return sqrt(x * x + y * y)
end

# =============================================================================
# Fused Multiply-Add functions
# =============================================================================

# fma: fused multiply-add, computes x*y+z without intermediate rounding.
#
# IEEE-compliant fused semantics on Float64 are provided by the internal
# `_fma` intrinsic (Issue #3732). Other numeric types fall through to the
# plain `x*y + z` formula via the generic method.
function fma(x::Float64, y::Float64, z::Float64)
    return _fma(x, y, z)
end

function fma(x, y, z)
    return x * y + z
end

# muladd: multiply-add, computes x*y+z.
# Unlike fma, muladd may or may not fuse the multiply-add depending on hardware,
# so a plain `x*y + z` is a valid implementation in Pure Julia.
function muladd(x, y, z)
    return x * y + z
end

# deg2rad: convert degrees to radians
function deg2rad(x)
    return x * 3.141592653589793 / 180.0
end

# rad2deg: convert radians to degrees
function rad2deg(x)
    return Float64(x) * 180.0 / 3.141592653589793
end

# iseven: check if integer is even
function iseven(x)
    return x % 2 == 0
end

# isodd: check if integer is odd
function isodd(x)
    return x % 2 != 0
end

# Note: abs(x) is implemented as a builtin (uses Intrinsic::AbsFloat)

# rem: remainder (same as % operator)
function rem(x, y)
    return x % y
end

# fld: floored division - returns integer type for integers, float for floats
# Julia's fld returns the same type as input for integers
function fld(x::Int64, y::Int64)
    # floor() returns Float64, convert back to Int64
    return Int64(floor(x / y))
end

function fld(x::Float64, y::Float64)
    return floor(x / y)
end

function fld(x, y)
    return floor(x / y)
end

# =============================================================================
# Trigonometric functions - derived forms
# =============================================================================

# sinpi/cospi: sin(π*x) / cos(π*x), accurate (and exact at integer & half-integer
# x) instead of the naive sin(pi*x)/cos(pi*x). Ported from Base.Math
# (julia/base/special/trig.jl, Issue #8309): reduce x to the interval [0, 0.5],
# then evaluate a minimax polynomial kernel on the small remainder rx ∈ [0, 0.25].

# Minimax kernel for sin(π*x) on x ∈ [0, 0.25] (Float64 coefficients from Base).
function _sinpi_kernel(x::Float64)
    x2 = x * x
    c1 = 3.141592653589793
    c1_lo = 1.2267151843884804e-16
    c3 = -5.16771278004997
    r = evalpoly(x2, (2.550164039877393, -0.5992645293247603, 0.082145886770189,
                      -0.007370434116378644, 0.000466329949762989, -2.1925990105975317e-5))
    return muladd(c1, x, x * muladd(c3, x2, muladd(x2 * x2, r, c1_lo)))
end

# Minimax kernel for cos(π*x) on x ∈ [0, 0.25] (Float64 coefficients from Base).
function _cospi_kernel(x::Float64)
    x2 = x * x
    c0 = 1.0
    c2 = 4.934802200544679          # -(sch.c₂ hi)
    c2_lo = 2.6451348079795815e-16  # -(sch.c₂ lo)
    r = evalpoly(x2, (4.058712126416749, -1.3352627688519947, 0.2353306301924776,
                      -0.025806885661227713, 0.0019294656071154924, -0.00010356606727649327)) * x2
    a_x2 = c2 * x2
    a_x2_lo = muladd(c2_lo, x2, muladd(c2, x2, -a_x2))
    w = c0 - a_x2
    return w + muladd(x2, r, ((c0 - w) - a_x2) - a_x2_lo)
end

function _sinpi_float(_x::Float64)
    x = abs(_x)
    if !isfinite(x)
        if isnan(x)
            return x
        end
        throw(DomainError(x, "`sinpi(x)` is only defined for finite `x`."))
    end
    # For large x, the answer is exactly 1 or 0.
    if x >= maxintfloat(Float64)
        return copysign(0.0, _x)
    end
    n = round(2.0 * x)            # reduce to [0, 0.5]
    rx = muladd(-0.5, n, x)
    q = Int64(n) & 3
    if q == 0
        res = _sinpi_kernel(rx)
    elseif q == 1
        res = _cospi_kernel(rx)
    elseif q == 2
        res = 0.0 - _sinpi_kernel(rx)
    else
        res = 0.0 - _cospi_kernel(rx)
    end
    return ifelse(signbit(_x), -res, res)
end

function _cospi_float(x::Float64)
    x = abs(x)
    if !isfinite(x)
        if isnan(x)
            return x
        end
        throw(DomainError(x, "`cospi(x)` is only defined for finite `x`."))
    end
    if x >= maxintfloat(Float64)
        return 1.0
    end
    n = round(2.0 * x)
    rx = muladd(-0.5, n, x)
    q = Int64(n) & 3
    if q == 0
        return _cospi_kernel(rx)
    elseif q == 1
        return 0.0 - _sinpi_kernel(rx)
    elseif q == 2
        return 0.0 - _cospi_kernel(rx)
    else
        return _sinpi_kernel(rx)
    end
end

# Integer arguments are exact: sinpi(n) = ±0, cospi(n) = ±1.
sinpi(x::Integer) = x >= 0 ? 0.0 : -0.0
cospi(x::Integer) = isodd(x) ? -1.0 : 1.0

sinpi(x::Real) = _sinpi_float(Float64(x))
cospi(x::Real) = _cospi_float(Float64(x))

# Fallback for non-real numeric types (e.g. Complex): keep the naive form.
sinpi(x) = sin(pi * x)
cospi(x) = cos(pi * x)

# sinc: normalized sinc function, sin(πx)/(πx), equals 1 at x=0
function sinc(x)
    if x == 0
        return 1.0
    end
    px = pi * x
    return sin(px) / px
end

# Concrete Float64 fast path (Issue #6846): a typed method lets the VM specialize
# the `==`, `*`, `sin`, `/` operations instead of dispatching them dynamically on
# an unknown-typed argument — ~1.5x faster than the generic `sinc(x)` above.
function sinc(x::Float64)
    if x == 0.0
        return 1.0
    end
    px = pi * x
    return sin(px) / px
end

# cosc: derivative of sinc, cos(πx)/x - sin(πx)/(πx²)
function cosc(x)
    if x == 0
        return 0.0
    end
    px = pi * x
    return cos(px) / x - sin(px) / (px * x)
end

# Concrete Float64 fast path (Issue #6846): a typed method lets the VM specialize
# the generic scalar arithmetic (`==`, `*`, `/`, `-`) instead of dispatching it
# dynamically on an unknown-typed argument — ~1.4x faster than the generic
# `cosc(x)` above (companion to the `sinc(x::Float64)` fast path).
function cosc(x::Float64)
    if x == 0.0
        return 0.0
    end
    px = pi * x
    return cos(px) / x - sin(px) / (px * x)
end

# sincos: return (sin(x), cos(x)) as a tuple
function sincos(x)
    return (sin(x), cos(x))
end

# Concrete Float64 fast path (Issue #6846): the typed method specializes the two
# transcendental dispatches and the tuple construction — ~2.3x faster than the
# generic `sincos(x)` above.
function sincos(x::Float64)
    return (sin(x), cos(x))
end

# sincospi: sine and cosine of pi*x simultaneously. Shares one range reduction,
# ported from Base.Math (julia/base/special/trig.jl, Issue #8309).
function _sincospi_float(_x::Float64)
    x = abs(_x)
    if !isfinite(x)
        if isnan(x)
            return (x, x)
        end
        throw(DomainError(x, "`sincospi(x)` is only defined for finite `x`."))
    end
    if x >= maxintfloat(Float64)
        return (copysign(0.0, _x), 1.0)
    end
    n = round(2.0 * x)
    rx = muladd(-0.5, n, x)
    q = Int64(n) & 3
    si = _sinpi_kernel(rx)
    co = _cospi_kernel(rx)
    if q == 1
        si, co = co, 0.0 - si
    elseif q == 2
        si, co = 0.0 - si, 0.0 - co
    elseif q == 3
        si, co = 0.0 - co, si
    end
    si = ifelse(signbit(_x), -si, si)
    return (si, co)
end

sincospi(x::Integer) = (sinpi(x), cospi(x))
sincospi(x::Real) = _sincospi_float(Float64(x))
sincospi(x) = (sinpi(x), cospi(x))

# tanpi: tangent of π*x, more accurate than tan(pi*x) for some cases
function tanpi(x)
    # For integers, tan(π*x) = 0
    if isinteger(x)
        return copysign(0.0, x)
    # For half-integers (x = n + 0.5), tan(π*x) = ±Inf
    elseif isinteger(2.0 * x) && !isinteger(x)
        return copysign(Inf, x)
    else
        return tan(pi * x)
    end
end

# Concrete Float64 fast path (Issue #6846): specializes the branch predicates and
# `tan(pi*x)` dispatch on a typed argument — modest (~1.1x) since `isinteger` /
# `copysign` dominate, but consistent with the rest of the trig fast paths.
function tanpi(x::Float64)
    if isinteger(x)
        return copysign(0.0, x)
    elseif isinteger(2.0 * x) && !isinteger(x)
        return copysign(Inf, x)
    else
        return tan(pi * x)
    end
end

# =============================================================================
# Degree-based trigonometric functions
# =============================================================================

# sind: sine of x in degrees
function sind(x)
    return sin(deg2rad(x))
end

# cosd: cosine of x in degrees
function cosd(x)
    return cos(deg2rad(x))
end

# tand: tangent of x in degrees
function tand(x)
    return tan(deg2rad(x))
end

# asind: arcsine returning degrees
function asind(x)
    return rad2deg(asin(x))
end

# acosd: arccosine returning degrees
function acosd(x)
    return rad2deg(acos(x))
end

# atand: arctangent returning degrees
function atand(x)
    return rad2deg(atan(x))
end

# sincosd: sine and cosine of x in degrees simultaneously
function sincosd(x)
    return (sind(x), cosd(x))
end

# =============================================================================
# Reciprocal trigonometric functions
# =============================================================================

# sec: secant, 1/cos(x)
function sec(x)
    return 1.0 / cos(x)
end

# csc: cosecant, 1/sin(x)
function csc(x)
    return 1.0 / sin(x)
end

# cot: cotangent, 1/tan(x) = cos(x)/sin(x)
function cot(x)
    return cos(x) / sin(x)
end

# asec: inverse secant, acos(1/x)
function asec(x)::Float64
    return acos(1.0 / x)
end

# acsc: inverse cosecant, asin(1/x)
function acsc(x)::Float64
    return asin(1.0 / x)
end

# acot: inverse cotangent, atan(1/x)
function acot(x)::Float64
    return atan(1.0 / x)
end

# =============================================================================
# Reciprocal hyperbolic functions
# =============================================================================

# sech: hyperbolic secant, 1/cosh(x)
function sech(x)
    return 1.0 / cosh(x)
end

# csch: hyperbolic cosecant, 1/sinh(x)
function csch(x)
    return 1.0 / sinh(x)
end

# coth: hyperbolic cotangent, cosh(x)/sinh(x)
function coth(x)
    return cosh(x) / sinh(x)
end

# =============================================================================
# Inverse reciprocal hyperbolic functions
# =============================================================================

# asech: inverse hyperbolic secant, acosh(1/x)
function asech(x)
    return acosh(1.0 / x)
end

# acsch: inverse hyperbolic cosecant, asinh(1/x)
function acsch(x)
    return asinh(1.0 / x)
end

# acoth: inverse hyperbolic cotangent, atanh(1/x)
function acoth(x)
    return atanh(1.0 / x)
end

# =============================================================================
# Degree-based reciprocal trigonometric functions
# =============================================================================

# secd: secant of x in degrees
function secd(x)
    return 1.0 / cosd(x)
end

# cscd: cosecant of x in degrees
function cscd(x)
    return 1.0 / sind(x)
end

# cotd: cotangent of x in degrees
function cotd(x)
    return cosd(x) / sind(x)
end

# asecd: inverse secant returning degrees
function asecd(x)
    return rad2deg(asec(x))
end

# acscd: inverse cosecant returning degrees
function acscd(x)
    return rad2deg(acsc(x))
end

# acotd: inverse cotangent returning degrees
function acotd(x)
    return rad2deg(acot(x))
end

# =============================================================================
# Division and modulo functions
# =============================================================================

# divrem: return (div(x,y), rem(x,y)) as a tuple
function divrem(x, y)
    return (div(x, y), rem(x, y))
end

# fldmod: return (fld(x,y), mod(x,y)) as a tuple
function fldmod(x, y)
    return (fld(x, y), mod(x, y))
end

# mod1: modulo with 1-based result (result in 1:y instead of 0:y-1)
function mod1(x, y)
    m = mod(x, y)
    if m == 0
        return y
    else
        return m
    end
end

# fld1: floored division adjusted for mod1
function fld1(x, y)
    return fld(x - 1, y)
end

# fldmod1: return (fld1(x,y), mod1(x,y)) as a tuple
function fldmod1(x, y)
    return (fld1(x, y), mod1(x, y))
end

# mod2pi: modulo 2π, result in [0, 2π)
function mod2pi(x::AbstractFloat)
    p2 = 2.0 * Float64(pi)
    r = mod(x, p2)
    if r == 0.0 && x > 0.0
        return p2
    else
        return r
    end
end

function mod2pi(x)
    return mod(Float64(x), 2.0 * Float64(pi))
end

# rem2pi: remainder after division by 2π, result in [-π, π]
function rem2pi(x)
    p = Float64(pi)
    r = mod(Float64(x), 2.0 * p)
    if r > p
        return r - 2.0 * p
    else
        return r
    end
end

# evalpoly: evaluate polynomial using Horner's method
# evalpoly(x, (a0, a1, a2, ...)) = a0 + a1*x + a2*x^2 + ...
function evalpoly(x, coeffs)
    n = length(coeffs)
    if n == 0
        return 0.0
    end
    result = coeffs[n]
    i = n - 1
    while i >= 1
        result = result * x + coeffs[i]
        i = i - 1
    end
    return result
end

# =============================================================================
# Miscellaneous math functions
# =============================================================================

# minmax: return (min, max) of two values
function minmax(a, b)
    if a <= b
        return (a, b)
    else
        return (b, a)
    end
end

# copysign: return |x| with the sign of y
function copysign(x, y)
    ax = abs(x)
    if y < 0
        return -ax
    else
        return ax
    end
end

# Note: flipsign is now defined in number.jl as the generic fallback

# =============================================================================
# Logarithmic functions
# =============================================================================

# log(b, x): logarithm of x with base b (Issue #2175)
# Based on Julia's base/math.jl: log(b::T, x::T) where {T<:Number} = log(x)/log(b)
function log(b, x)
    y = log(x) / log(b)
    nearest = round(y)
    if abs(y - nearest) < 1.0e-12
        return nearest
    end
    return y
end

# log2: logarithm base 2
function log2(x)
    return log(x) / log(2.0)
end

# log10: logarithm base 10
function log10(x)
    return log(x) / log(10.0)
end

# log1p: log(1 + x), more accurate for small x
function log1p(x)
    return log(1.0 + x)
end

# expm1: exp(x) - 1, more accurate for small x (Issue #2095)
# Based on Julia's base/math.jl
function expm1(x)
    return exp(x) - 1.0
end

# =============================================================================
# Hyperbolic functions (derived from exp)
# =============================================================================

# sinh: hyperbolic sine, (exp(x) - exp(-x)) / 2
function sinh(x)
    return (exp(x) - exp(-x)) / 2.0
end

# cosh: hyperbolic cosine, (exp(x) + exp(-x)) / 2
function cosh(x)
    return (exp(x) + exp(-x)) / 2.0
end

# tanh: hyperbolic tangent, sinh(x) / cosh(x)
function tanh(x)
    ex = exp(x)
    emx = exp(-x)
    return (ex - emx) / (ex + emx)
end

# asinh: inverse hyperbolic sine, log(x + sqrt(x^2 + 1))
function asinh(x)
    return log(x + sqrt(x * x + 1.0))
end

# acosh: inverse hyperbolic cosine, log(x + sqrt(x^2 - 1))
function acosh(x)
    return log(x + sqrt(x * x - 1.0))
end

# atanh: inverse hyperbolic tangent, log((1+x) / (1-x)) / 2
function atanh(x)
    return log((1.0 + x) / (1.0 - x)) / 2.0
end

# =============================================================================
# Exponential functions (base 2 and base 10)
# =============================================================================
# Based on Julia's base/math.jl:1343-1344

# exp2: 2^x, exponential base 2
function exp2(x)
    return 2.0 ^ x
end

# exp10: 10^x, exponential base 10
function exp10(x)
    return 10.0 ^ x
end

# =============================================================================
# Cube root (Issue #1857)
# =============================================================================
# Based on Julia's base/special/cbrt.jl:34

# cbrt: cube root, x^(1/3), handles negative values
function cbrt(x::Float64)
    if x < 0.0
        return -((-x) ^ (1.0 / 3.0))
    else
        return x ^ (1.0 / 3.0)
    end
end

function cbrt(x::Float32)
    if x < Float32(0.0)
        return -((-x) ^ Float32(1.0 / 3.0))
    else
        return x ^ Float32(1.0 / 3.0)
    end
end

function cbrt(x::Int64)
    return cbrt(Float64(x))
end

# Rational conversion (Issue #5356): `float(::Rational)` is Float64, reducing to
# the Float64 method (no recursion).
function cbrt(x::Rational)
    return cbrt(float(x))
end

# =============================================================================
# Fourth root (Issue #1859)
# =============================================================================
# Based on Julia's base/math.jl:698

# fourthroot: fourth root, x^(1/4) = sqrt(sqrt(x))
function fourthroot(x::Float64)
    return sqrt(sqrt(x))
end

function fourthroot(x::Float32)
    return sqrt(sqrt(x))
end

function fourthroot(x::Int64)
    return fourthroot(Float64(x))
end
