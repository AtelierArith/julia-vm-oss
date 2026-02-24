# =============================================================================
# prelude_aot.jl - Minimal Typed Prelude for AoT Compilation
# =============================================================================
# This file contains a minimal set of fully-typed functions designed for
# pure Rust code generation without runtime Value type dependency.
#
# All functions have explicit type annotations to enable static dispatch.
# No dynamic dispatch or runtime type checking is used.
#
# Usage: cargo run --bin aot -- --minimal-prelude input.jl
# =============================================================================

# =============================================================================
# Integer Arithmetic (Int64)
# =============================================================================

function Base.:+(x::Int64, y::Int64)::Int64
    add_int(x, y)
end

function Base.:-(x::Int64, y::Int64)::Int64
    sub_int(x, y)
end

function Base.:-(x::Int64)::Int64
    sub_int(0, x)
end

function Base.:*(x::Int64, y::Int64)::Int64
    mul_int(x, y)
end

function div(x::Int64, y::Int64)::Int64
    sdiv_int(x, y)
end

function rem(x::Int64, y::Int64)::Int64
    srem_int(x, y)
end

function Base.:%(x::Int64, y::Int64)::Int64
    srem_int(x, y)
end

# =============================================================================
# Integer Comparisons (Int64)
# =============================================================================

function Base.:(==)(x::Int64, y::Int64)::Bool
    eq_int(x, y)
end

function Base.:(!=)(x::Int64, y::Int64)::Bool
    ne_int(x, y)
end

function Base.:(<)(x::Int64, y::Int64)::Bool
    slt_int(x, y)
end

function Base.:(<=)(x::Int64, y::Int64)::Bool
    sle_int(x, y)
end

function Base.:(>)(x::Int64, y::Int64)::Bool
    sgt_int(x, y)
end

function Base.:(>=)(x::Int64, y::Int64)::Bool
    sge_int(x, y)
end

# =============================================================================
# Integer Bitwise Operations (Int64)
# =============================================================================

function Base.:(&)(x::Int64, y::Int64)::Int64
    and_int(x, y)
end

function Base.:(|)(x::Int64, y::Int64)::Bool
    or_int(x, y)
end

function Base.:(~)(x::Int64)::Int64
    not_int(x)
end

function Base.xor(x::Int64, y::Int64)::Int64
    xor_int(x, y)
end

function Base.:(<<)(x::Int64, y::Int64)::Int64
    shl_int(x, y)
end

function Base.:(>>)(x::Int64, y::Int64)::Int64
    ashr_int(x, y)
end

function Base.:(>>>)(x::Int64, y::Int64)::Int64
    lshr_int(x, y)
end

# =============================================================================
# Float64 Arithmetic
# =============================================================================

function Base.:+(x::Float64, y::Float64)::Float64
    add_float(x, y)
end

function Base.:-(x::Float64, y::Float64)::Float64
    sub_float(x, y)
end

function Base.:-(x::Float64)::Float64
    neg_float(x)
end

function Base.:*(x::Float64, y::Float64)::Float64
    mul_float(x, y)
end

function Base.:/(x::Float64, y::Float64)::Float64
    div_float(x, y)
end

function Base.:^(x::Float64, y::Float64)::Float64
    pow_float(x, y)
end

# =============================================================================
# Float64 Comparisons
# =============================================================================

function Base.:(==)(x::Float64, y::Float64)::Bool
    eq_float(x, y)
end

function Base.:(!=)(x::Float64, y::Float64)::Bool
    ne_float(x, y)
end

function Base.:(<)(x::Float64, y::Float64)::Bool
    lt_float(x, y)
end

function Base.:(<=)(x::Float64, y::Float64)::Bool
    le_float(x, y)
end

function Base.:(>)(x::Float64, y::Float64)::Bool
    gt_float(x, y)
end

function Base.:(>=)(x::Float64, y::Float64)::Bool
    ge_float(x, y)
end

# =============================================================================
# Float64 Math Functions
# =============================================================================

function sqrt(x::Float64)::Float64
    sqrt_llvm(x)
end

function floor(x::Float64)::Float64
    floor_llvm(x)
end

function ceil(x::Float64)::Float64
    ceil_llvm(x)
end

function trunc(x::Float64)::Float64
    trunc_llvm(x)
end

function abs(x::Float64)::Float64
    abs_float(x)
end

function copysign(x::Float64, y::Float64)::Float64
    copysign_float(x, y)
end

# =============================================================================
# Type Conversions
# =============================================================================

# Int64 to Float64
function Float64(x::Int64)::Float64
    sitofp(Float64, x)
end

# Float64 to Int64 (truncate)
function Int64(x::Float64)::Int64
    fptosi(Int64, x)
end

# =============================================================================
# Boolean Operations
# =============================================================================

function Base.:(!)(x::Bool)::Bool
    if x
        false
    else
        true
    end
end

function Base.:(&)(x::Bool, y::Bool)::Bool
    if x
        y
    else
        false
    end
end

function Base.:(|)(x::Bool, y::Bool)::Bool
    if x
        true
    else
        y
    end
end

function xor(x::Bool, y::Bool)::Bool
    if x
        if y
            false
        else
            true
        end
    else
        y
    end
end

# =============================================================================
# Integer Utility Functions
# =============================================================================

function abs(x::Int64)::Int64
    if x < 0
        0 - x
    else
        x
    end
end

function sign(x::Int64)::Int64
    if x > 0
        1
    elseif x < 0
        -1
    else
        0
    end
end

function sign(x::Float64)::Float64
    if x > 0.0
        1.0
    elseif x < 0.0
        -1.0
    else
        0.0
    end
end

function min(x::Int64, y::Int64)::Int64
    if x < y
        x
    else
        y
    end
end

function max(x::Int64, y::Int64)::Int64
    if x > y
        x
    else
        y
    end
end

function min(x::Float64, y::Float64)::Float64
    if x < y
        x
    else
        y
    end
end

function max(x::Float64, y::Float64)::Float64
    if x > y
        x
    else
        y
    end
end

function clamp(x::Int64, lo::Int64, hi::Int64)::Int64
    if x < lo
        lo
    elseif x > hi
        hi
    else
        x
    end
end

function clamp(x::Float64, lo::Float64, hi::Float64)::Float64
    if x < lo
        lo
    elseif x > hi
        hi
    else
        x
    end
end

# =============================================================================
# Mixed-Type Arithmetic (Int64 + Float64)
# =============================================================================

function Base.:+(x::Int64, y::Float64)::Float64
    add_float(Float64(x), y)
end

function Base.:+(x::Float64, y::Int64)::Float64
    add_float(x, Float64(y))
end

function Base.:-(x::Int64, y::Float64)::Float64
    sub_float(Float64(x), y)
end

function Base.:-(x::Float64, y::Int64)::Float64
    sub_float(x, Float64(y))
end

function Base.:*(x::Int64, y::Float64)::Float64
    mul_float(Float64(x), y)
end

function Base.:*(x::Float64, y::Int64)::Float64
    mul_float(x, Float64(y))
end

function Base.:/(x::Int64, y::Float64)::Float64
    div_float(Float64(x), y)
end

function Base.:/(x::Float64, y::Int64)::Float64
    div_float(x, Float64(y))
end

function Base.:/(x::Int64, y::Int64)::Float64
    div_float(Float64(x), Float64(y))
end

# =============================================================================
# Number Predicates
# =============================================================================

function iszero(x::Int64)::Bool
    x == 0
end

function iszero(x::Float64)::Bool
    x == 0.0
end

function isone(x::Int64)::Bool
    x == 1
end

function isone(x::Float64)::Bool
    x == 1.0
end

function iseven(x::Int64)::Bool
    (x % 2) == 0
end

function isodd(x::Int64)::Bool
    (x % 2) != 0
end

function ispositive(x::Int64)::Bool
    x > 0
end

function ispositive(x::Float64)::Bool
    x > 0.0
end

function isnegative(x::Int64)::Bool
    x < 0
end

function isnegative(x::Float64)::Bool
    x < 0.0
end

# =============================================================================
# Identity Function
# =============================================================================

function identity(x::Int64)::Int64
    x
end

function identity(x::Float64)::Float64
    x
end

function identity(x::Bool)::Bool
    x
end

# =============================================================================
# GCD and LCM (Int64)
# =============================================================================

function gcd(a::Int64, b::Int64)::Int64
    a = abs(a)
    b = abs(b)
    while b != 0
        t = b
        b = a % b
        a = t
    end
    a
end

function lcm(a::Int64, b::Int64)::Int64
    if a == 0 || b == 0
        0
    else
        div(abs(a * b), gcd(a, b))
    end
end

# =============================================================================
# Power (Integer Exponent)
# =============================================================================

function Base.:^(x::Int64, n::Int64)::Int64
    if n == 0
        1
    elseif n == 1
        x
    elseif n < 0
        0  # Integer division truncates to zero for negative exponents
    else
        result = 1
        base = x
        exp = n
        while exp > 0
            if (exp % 2) == 1
                result = result * base
            end
            base = base * base
            exp = div(exp, 2)
        end
        result
    end
end

function Base.:^(x::Float64, n::Int64)::Float64
    if n == 0
        1.0
    elseif n == 1
        x
    elseif n < 0
        1.0 / (x ^ (-n))
    else
        result = 1.0
        base = x
        exp = n
        while exp > 0
            if (exp % 2) == 1
                result = result * base
            end
            base = base * base
            exp = div(exp, 2)
        end
        result
    end
end

# =============================================================================
# Factorial
# =============================================================================

function factorial(n::Int64)::Int64
    if n < 0
        error("factorial requires non-negative argument")
    elseif n == 0 || n == 1
        1
    else
        result = 1
        i = 2
        while i <= n
            result = result * i
            i = i + 1
        end
        result
    end
end

# =============================================================================
# Complex (AoT minimal)
# =============================================================================

struct Complex
    re::Float64
    im::Float64
end

function Base.:+(x::Complex, y::Complex)::Complex
    Complex(x.re + y.re, x.im + y.im)
end

function Base.:+(x::Float64, y::Complex)::Complex
    Complex(x + y.re, y.im)
end

function Base.:+(x::Complex, y::Float64)::Complex
    Complex(x.re + y, x.im)
end

function Base.:*(x::Complex, y::Complex)::Complex
    Complex(x.re * y.re - x.im * y.im, x.re * y.im + x.im * y.re)
end

function Base.:*(x::Complex, y::Float64)::Complex
    Complex(x.re * y, x.im * y)
end

function Base.:*(x::Float64, y::Complex)::Complex
    Complex(x * y.re, x * y.im)
end

function Base.:^(x::Complex, n::Int64)::Complex
    if n == 0
        Complex(1.0, 0.0)
    elseif n == 1
        x
    elseif n == 2
        x * x
    elseif n < 0
        Complex(0.0, 0.0)
    else
        result = Complex(1.0, 0.0)
        base = x
        exp = n
        while exp > 0
            if (exp % 2) == 1
                result = result * base
            end
            base = base * base
            exp = div(exp, 2)
        end
        result
    end
end

function abs2(z::Complex)::Float64
    z.re * z.re + z.im * z.im
end

# =============================================================================
# Range / Adjoint helpers for broadcast patterns
# =============================================================================

function range(start::Float64, stop::Float64, length::Int64)::Vector{Float64}
    if length <= 0
        Float64[]
    elseif length == 1
        [start]
    else
        step = (stop - start) / Float64(length - 1)
        result = zeros(length)
        i = 1
        while i <= length
            result[i] = start + Float64(i - 1) * step
            i = i + 1
        end
        result
    end
end

function adjoint(v::Vector{Float64})::Matrix{Float64}
    n::Int64 = length(v)
    row::Matrix{Float64} = zeros(1, n)
    i::Int64 = 1
    while i <= n
        row[1, i] = v[i]
        i = i + 1
    end
    row
end
