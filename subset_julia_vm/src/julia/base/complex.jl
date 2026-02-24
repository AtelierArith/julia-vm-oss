# =============================================================================
# Complex - Complex number type (Julia-compatible implementation)
# =============================================================================
# Based on Julia's base/complex.jl
#
# Complex numbers are represented as re + im*i where re and im are real numbers.
#
# IMPORTANT: This module uses Julia-standard operator overloading.
# All arithmetic uses Base.:+ style extensions.

# Complex number struct - parametric type matching official Julia
# Complex is a subtype of Number (enables numeric dispatch)
struct Complex{T<:Real} <: Number
    re::T
    im::T
end

# Type aliases matching official Julia
const ComplexF64 = Complex{Float64}
const ComplexF32 = Complex{Float32}

# =============================================================================
# Constructors
# =============================================================================

# Two-argument constructors (explicit types)
function Complex(x::Int64, y::Int64)
    return Complex{Int64}(x, y)
end

function Complex(x::Float64, y::Float64)
    return Complex{Float64}(x, y)
end

function Complex(x::Int64, y::Float64)
    return Complex{Float64}(Float64(x), y)
end

function Complex(x::Float64, y::Int64)
    return Complex{Float64}(x, Float64(y))
end

function Complex(x::Bool, y::Bool)
    return Complex{Bool}(x, y)
end

# Single argument constructors (use Complex function, not Complex{T})
function Complex(x::Int64)
    return Complex{Int64}(x, Int64(0))
end

function Complex(x::Float64)
    return Complex{Float64}(x, 0.0)
end

function Complex(x::Bool)
    return Complex{Bool}(x, false)
end

# Float32 constructors
function Complex(x::Float32, y::Float32)
    return Complex{Float32}(x, y)
end

function Complex(x::Float32)
    return Complex{Float32}(x, Float32(0.0))
end

function Complex(x::Int64, y::Float32)
    return Complex{Float32}(Float32(x), y)
end

function Complex(x::Float32, y::Int64)
    return Complex{Float32}(x, Float32(y))
end

function Complex(x::Float64, y::Float32)
    return Complex{Float64}(x, Float64(y))
end

function Complex(x::Float32, y::Float64)
    return Complex{Float64}(Float64(x), y)
end

# Generic 2-argument constructor: promote types when they differ
Complex(x::Real, y::Real) = Complex(promote(x, y)...)

# Generic parametric 1-argument constructor
function Complex{T}(x::Real) where {T<:Real}
    Complex{T}(convert(T, x), zero(T))
end

# Complex→Complex conversion constructor
function Complex{T}(z::Complex) where {T<:Real}
    Complex{T}(convert(T, real(z)), convert(T, imag(z)))
end

# Legacy function-style constructors
function complex(r::Int64, i::Int64)::Complex{Int64}
    return Complex{Int64}(r, i)
end

function complex(r::Float64, i::Float64)::Complex{Float64}
    return Complex{Float64}(r, i)
end

function complex(r::Int64)::Complex{Int64}
    return Complex{Int64}(r, Int64(0))
end

function complex(r::Float64)::Complex{Float64}
    return Complex{Float64}(r, 0.0)
end

function complex(r::Float32, i::Float32)::Complex{Float32}
    return Complex{Float32}(r, i)
end

function complex(r::Float32)::Complex{Float32}
    return Complex{Float32}(r, Float32(0.0))
end

# =============================================================================
# The imaginary unit constant
# In official Julia: const im = Complex{Bool}(false, true)
# =============================================================================
const im = Complex{Bool}(false, true)

# =============================================================================
# Accessor functions
# =============================================================================

# Generic accessor using parametric type - matches any Complex{T} where T<:Real
function real(z::Complex{T}) where T<:Real
    return z.re
end

function imag(z::Complex{T}) where T<:Real
    return z.im
end

# Real numbers: real(x) = x, imag(x) = 0
function real(x::Real)
    return x
end

function imag(x::Real)
    return zero(x)
end

# =============================================================================
# Predicates
# =============================================================================

function iszero(z::Complex{T}) where T<:Real
    return z.re == zero(z.re) && z.im == zero(z.im)
end

function isreal(z::Complex{T}) where T<:Real
    return z.im == zero(z.im)
end

# isfinite: true if both real and imaginary parts are finite
# Based on Julia's base/complex.jl:150
function isfinite(z::Complex{T}) where T<:Real
    return isfinite(z.re) && isfinite(z.im)
end

# isnan: true if either real or imaginary part is NaN
# Based on Julia's base/complex.jl:151
function isnan(z::Complex{T}) where T<:Real
    return isnan(z.re) || isnan(z.im)
end

# isinf: true if either real or imaginary part is infinite
# Based on Julia's base/complex.jl:152
function isinf(z::Complex{T}) where T<:Real
    return isinf(z.re) || isinf(z.im)
end

# =============================================================================
# Unary operators
# =============================================================================

# Negation: -z
function Base.:-(z::Complex{T}) where {T<:Real}
    return Complex{T}(-z.re, -z.im)
end

# Complex conjugate: conj(z)
function conj(z::Complex{T}) where {T<:Real}
    return Complex(z.re, -z.im)
end

# For real numbers, conjugate is identity (Julia behavior)
function conj(x::Float64)
    return x
end

function conj(x::Int64)
    return x
end

# =============================================================================
# adjoint and transpose for Complex scalars
# =============================================================================
# Based on Julia's base/number.jl:268-269
# Issue #933 (VM bug) has been fixed, so Pure Julia implementation is now used.

# adjoint(z::Complex) = conj(z)
# For complex numbers, adjoint is the complex conjugate
function adjoint(z::Complex{T}) where {T<:Real}
    return conj(z)
end

# transpose(z::Complex) = z
# For scalars, transpose is identity
function transpose(z::Complex{T}) where {T<:Real}
    return z
end

# Squared magnitude: abs2(z) = |z|^2 = re^2 + im^2
function abs2(z::Complex{T}) where {T<:Real}
    return z.re * z.re + z.im * z.im
end

# Magnitude: abs(z) = |z| = sqrt(re^2 + im^2)
function abs(z::Complex{T}) where {T<:Real}
    return sqrt(Float64(z.re * z.re + z.im * z.im))
end

# =============================================================================
# Binary arithmetic operators for Complex types
# Based on Julia's base/complex.jl - these handle any type combination
# via promotion through the Complex constructor.
# =============================================================================

# Complex + Complex (generic, handles mixed-type via promotion in constructor)
Base.:+(z::Complex, w::Complex) = Complex(real(z) + real(w), imag(z) + imag(w))
Base.:-(z::Complex, w::Complex) = Complex(real(z) - real(w), imag(z) - imag(w))
Base.:*(z::Complex, w::Complex) = Complex(real(z)*real(w) - imag(z)*imag(w),
                                          real(z)*imag(w) + imag(z)*real(w))

# Complex + Real (generic)
Base.:+(x::Real, z::Complex) = Complex(x + real(z), imag(z))
Base.:+(z::Complex, x::Real) = Complex(real(z) + x, imag(z))
Base.:-(x::Real, z::Complex) = Complex(x - real(z), -imag(z))
Base.:-(z::Complex, x::Real) = Complex(real(z) - x, imag(z))
Base.:*(x::Real, z::Complex) = Complex(x * real(z), x * imag(z))
Base.:*(z::Complex, x::Real) = Complex(real(z) * x, imag(z) * x)

# Division - generic (always returns Float64 for numerical stability)
function Base.:/(z::Complex, w::Complex)
    a = Float64(real(z))
    b = Float64(imag(z))
    c = Float64(real(w))
    d = Float64(imag(w))
    denom = c * c + d * d
    Complex{Float64}((a * c + b * d) / denom, (b * c - a * d) / denom)
end

function Base.:/(z::Complex, x::Real)
    Complex(real(z) / x, imag(z) / x)
end

function Base.:/(x::Real, z::Complex)
    fx = Float64(x)
    c = Float64(real(z))
    d = Float64(imag(z))
    denom = c * c + d * d
    Complex{Float64}(fx * c / denom, -fx * d / denom)
end

# =============================================================================
# Comparison operators
# =============================================================================

# Generic where clause method for any Complex{T} == Complex{S}
function Base.:(==)(x::Complex{T}, y::Complex{S}) where {T<:Real, S<:Real}
    return x.re == y.re && x.im == y.im
end

function Base.:(!=)(x::Complex{T}, y::Complex{S}) where {T<:Real, S<:Real}
    return x.re != y.re || x.im != y.im
end

# Explicit == methods for same-type comparisons (for better dispatch)
function Base.:(==)(x::Complex{Float64}, y::Complex{Float64})
    return x.re == y.re && x.im == y.im
end

function Base.:(==)(x::Complex{Int64}, y::Complex{Int64})
    return x.re == y.re && x.im == y.im
end

function Base.:(==)(x::Complex{Bool}, y::Complex{Bool})
    return x.re == y.re && x.im == y.im
end

# Explicit == methods for cross-type comparisons
function Base.:(==)(x::Complex{Float64}, y::Complex{Int64})
    return Float64(x.re) == Float64(y.re) && Float64(x.im) == Float64(y.im)
end

function Base.:(==)(x::Complex{Int64}, y::Complex{Float64})
    return Float64(x.re) == Float64(y.re) && Float64(x.im) == Float64(y.im)
end

function Base.:(==)(x::Complex{Float64}, y::Complex{Bool})
    return x.re == Float64(y.re) && x.im == Float64(y.im)
end

function Base.:(==)(x::Complex{Bool}, y::Complex{Float64})
    return Float64(x.re) == y.re && Float64(x.im) == y.im
end

function Base.:(==)(x::Complex{Int64}, y::Complex{Bool})
    return x.re == Int64(y.re) && x.im == Int64(y.im)
end

function Base.:(==)(x::Complex{Bool}, y::Complex{Int64})
    return Int64(x.re) == y.re && Int64(x.im) == y.im
end

# Complex{Float32} comparison methods
function Base.:(==)(x::Complex{Float32}, y::Complex{Float32})
    return x.re == y.re && x.im == y.im
end

function Base.:(==)(x::Complex{Float32}, y::Complex{Float64})
    return Float64(x.re) == y.re && Float64(x.im) == y.im
end

function Base.:(==)(x::Complex{Float64}, y::Complex{Float32})
    return x.re == Float64(y.re) && x.im == Float64(y.im)
end

function Base.:(==)(x::Complex{Float32}, y::Complex{Int64})
    return Float32(x.re) == Float32(y.re) && Float32(x.im) == Float32(y.im)
end

function Base.:(==)(x::Complex{Int64}, y::Complex{Float32})
    return Float32(x.re) == Float32(y.re) && Float32(x.im) == Float32(y.im)
end

# =============================================================================
# Type identity functions (for array operations)
# =============================================================================

# Zero element for Complex (additive identity) - explicit types
function zero(::Complex{Float64})
    return Complex{Float64}(0.0, 0.0)
end

function zero(::Complex{Int64})
    return Complex{Int64}(Int64(0), Int64(0))
end

function zero(::Complex{Bool})
    return Complex{Bool}(false, false)
end

function zero(::Complex{Float32})
    return Complex{Float32}(Float32(0.0), Float32(0.0))
end

function zero(::Type{Complex{Float64}})
    return Complex{Float64}(0.0, 0.0)
end

function zero(::Type{Complex{Int64}})
    return Complex{Int64}(Int64(0), Int64(0))
end

function zero(::Type{Complex{Bool}})
    return Complex{Bool}(false, false)
end

function zero(::Type{Complex{Float32}})
    return Complex{Float32}(Float32(0.0), Float32(0.0))
end

# One element for Complex (multiplicative identity) - explicit types
function one(::Complex{Float64})
    return Complex{Float64}(1.0, 0.0)
end

function one(::Complex{Int64})
    return Complex{Int64}(Int64(1), Int64(0))
end

function one(::Complex{Bool})
    return Complex{Bool}(true, false)
end

function one(::Complex{Float32})
    return Complex{Float32}(Float32(1.0), Float32(0.0))
end

function one(::Type{Complex{Float64}})
    return Complex{Float64}(1.0, 0.0)
end

function one(::Type{Complex{Int64}})
    return Complex{Int64}(Int64(1), Int64(0))
end

function one(::Type{Complex{Bool}})
    return Complex{Bool}(true, false)
end

function one(::Type{Complex{Float32}})
    return Complex{Float32}(Float32(1.0), Float32(0.0))
end

# one for basic types
function one(::Type{Int64})
    Int64(1)
end

function one(::Type{Float64})
    1.0
end

function one(::Type{Bool})
    true
end

# =============================================================================
# Argument/Phase
# =============================================================================

# angle: argument (phase) of complex number in radians
function angle(z::Complex{Float64})
    return atan(z.im, z.re)
end

# =============================================================================
# Transcendental functions
# =============================================================================

# exp: complex exponential e^z = e^(x+iy) = e^x * (cos(y) + i*sin(y))
function exp(z::Complex{Float64})
    er = exp(z.re)
    return Complex{Float64}(er * cos(z.im), er * sin(z.im))
end

# log: complex logarithm log(z) = log|z| + i*arg(z)
function log(z::Complex{Float64})
    return Complex{Float64}(log(abs(z)), angle(z))
end

# sqrt: complex square root
# sqrt(z) = sqrt(|z|) * (cos(θ/2) + i*sin(θ/2)) where θ = arg(z)
function sqrt(z::Complex{Float64})
    r = abs(z)
    if r == 0.0
        return Complex{Float64}(0.0, 0.0)
    end
    # Use the formula that avoids computing angle explicitly
    # Re(sqrt(z)) = sqrt((|z| + Re(z)) / 2)
    # Im(sqrt(z)) = sign(Im(z)) * sqrt((|z| - Re(z)) / 2)
    re_part = sqrt((r + z.re) / 2.0)
    im_sign = z.im >= 0.0 ? 1.0 : -1.0
    im_part = im_sign * sqrt((r - z.re) / 2.0)
    return Complex{Float64}(re_part, im_part)
end

# =============================================================================
# Power functions
# =============================================================================

# Complex to complex power: z^w = exp(w * log(z))
function Base.:^(z::Complex{Float64}, w::Complex{Float64})
    if z.re == 0.0 && z.im == 0.0
        # 0^w = 0 for Re(w) > 0
        if w.re > 0.0
            return Complex{Float64}(0.0, 0.0)
        end
    end
    lz = log(z)
    p = w * lz
    return exp(p)
end

# Complex to real power: z^x = exp(x * log(z))
function Base.:^(z::Complex{Float64}, x::Real)
    fx = Float64(x)
    if z.re == 0.0 && z.im == 0.0
        if fx > 0.0
            return Complex{Float64}(0.0, 0.0)
        end
    end
    lz = log(z)
    cx = Complex{Float64}(fx, 0.0)
    w = cx * lz
    return exp(w)
end

# Real to complex power: x^w = exp(w * log(x))
function Base.:^(x::Real, w::Complex{Float64})
    fx = Float64(x)
    if fx > 0.0
        lx = Complex{Float64}(log(fx), 0.0)
        p = w * lx
        return exp(p)
    elseif fx == 0.0
        if w.re > 0.0
            return Complex{Float64}(0.0, 0.0)
        end
    end
    # For negative real base, convert to complex
    cz = Complex{Float64}(fx, 0.0)
    return cz ^ w
end

# =============================================================================
# Additional trigonometric functions for Complex
# =============================================================================

# sin(z) = (exp(iz) - exp(-iz)) / (2i)
function sin(z::Complex{Float64})
    iz = Complex{Float64}(-z.im, z.re)  # i * z
    e1 = exp(iz)
    e2 = exp(Complex{Float64}(z.im, -z.re))  # exp(-iz)
    # (e1 - e2) / (2i) = (e1 - e2) * (-i/2)
    diff = e1 - e2
    return Complex{Float64}(diff.im / 2.0, -diff.re / 2.0)
end

# cos(z) = (exp(iz) + exp(-iz)) / 2
function cos(z::Complex{Float64})
    iz = Complex{Float64}(-z.im, z.re)  # i * z
    e1 = exp(iz)
    e2 = exp(Complex{Float64}(z.im, -z.re))  # exp(-iz)
    sum_val = e1 + e2
    return Complex{Float64}(sum_val.re / 2.0, sum_val.im / 2.0)
end

# tan(z) = sin(z) / cos(z)
function tan(z::Complex{Float64})
    return sin(z) / cos(z)
end

# =============================================================================
# cis - complex exponential function
# =============================================================================
# Based on Julia's base/complex.jl
#
# cis(x) returns cos(x) + im*sin(x) = exp(im*x)
# More efficient than computing exp(im*x) directly

function cis(x::Real)
    return Complex{Float64}(cos(Float64(x)), sin(Float64(x)))
end

function cis(x::Int64)
    fx = Float64(x)
    return Complex{Float64}(cos(fx), sin(fx))
end

# cispi(x) = cis(π*x) = cos(πx) + im*sin(πx)
function cispi(x::Real)
    return Complex{Float64}(cospi(Float64(x)), sinpi(Float64(x)))
end

function cispi(x::Int64)
    fx = Float64(x)
    return Complex{Float64}(cospi(fx), sinpi(fx))
end

# =============================================================================
# reim - decompose complex number into (real, imag) tuple
# =============================================================================
# Based on Julia's base/complex.jl

function reim(z::Complex{T}) where T<:Real
    return (z.re, z.im)
end

function reim(x::Real)
    return (x, zero(x))
end

function reim(x::Int64)
    return (x, Int64(0))
end

function reim(x::Float64)
    return (x, 0.0)
end

# =============================================================================
# Type conversion: float
# =============================================================================

# float(z::Complex) - convert complex to Complex{Float64}
function float(z::Complex)
    return Complex{Float64}(float(real(z)), float(imag(z)))
end

# =============================================================================
# conj! - in-place conjugate for arrays
# =============================================================================
# Based on Julia's base/abstractarraymath.jl
#
# NOTE: Complex array support is limited due to VM constraints with
# setting complex values via indexed assignment. For real arrays,
# conjugate is identity (returns array unchanged).

# For arrays, apply conj to each element in-place
# For real arrays, this is effectively identity since conj(x) = x for reals
function conj!(A::Array)
    for i in 1:length(A)
        ai = A[i]
        A[i] = conj(ai)
    end
    return A
end
