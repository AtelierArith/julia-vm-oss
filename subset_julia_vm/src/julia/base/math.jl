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

# mod: modulo operation (result has same sign as divisor)
function mod(x, y)
    r = x % y
    if r != 0 && (r < 0) != (y < 0)
        return r + y
    else
        return r
    end
end

# div: integer division (floor division)
function div(x, y)
    return floor(x / y)
end

# hypot: hypotenuse length sqrt(x^2 + y^2)
function hypot(x, y)
    return sqrt(x * x + y * y)
end

# =============================================================================
# Fused Multiply-Add functions
# =============================================================================

# fma: fused multiply-add, computes x*y+z without intermediate rounding
# For real hardware fma, the intermediate x*y result is not rounded before adding z.
# This implementation uses the simple formula for compatibility.
# Note: For IEEE-compliant fma on Float64, a hardware intrinsic would be needed.
function fma(x, y, z)
    return x * y + z
end

# muladd: multiply-add, computes x*y+z
# Unlike fma, muladd may or may not fuse the multiply-add depending on hardware.
# This is typically faster than fma when exact fused behavior isn't required.
function muladd(x, y, z)
    return x * y + z
end

# deg2rad: convert degrees to radians
function deg2rad(x)
    return x * 3.141592653589793 / 180.0
end

# rad2deg: convert radians to degrees
function rad2deg(x)
    return x * 180.0 / 3.141592653589793
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

# sinpi: sin(π*x), more accurate than sin(pi*x)
function sinpi(x)
    return sin(pi * x)
end

# cospi: cos(π*x), more accurate than cos(pi*x)
function cospi(x)
    return cos(pi * x)
end

# sinc: normalized sinc function, sin(πx)/(πx), equals 1 at x=0
function sinc(x)
    if x == 0
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

# sincos: return (sin(x), cos(x)) as a tuple
function sincos(x)
    return (sin(x), cos(x))
end

# sincospi: sine and cosine of pi*x simultaneously
function sincospi(x)
    return (sinpi(x), cospi(x))
end

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
function asec(x)
    return acos(1.0 / x)
end

# acsc: inverse cosecant, asin(1/x)
function acsc(x)
    return asin(1.0 / x)
end

# acot: inverse cotangent, atan(1/x)
function acot(x)
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
function mod2pi(x)
    return mod(x, 2.0 * pi)
end

# rem2pi: remainder after division by 2π, result in [-π, π]
function rem2pi(x)
    r = mod(x, 2.0 * pi)
    if r > pi
        return r - 2.0 * pi
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
    return log(x) / log(b)
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
