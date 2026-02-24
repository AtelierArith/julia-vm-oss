# =============================================================================
# int.jl - Integer Arithmetic (Int64 specialized)
# =============================================================================
# Based on Julia's base/int.jl
# These specialized methods ensure Int64 operations return Int64.

# =============================================================================
# Number-theoretic Functions (Int64 specialized)
# =============================================================================

# Unary negation for Int64 - returns Int64 (not Float64)
# This ensures -x returns Int64 when x is Int64
function Base.:-(x::Int64)
    return 0 - x
end

# =============================================================================
# Sign-related functions (based on Julia's base/int.jl:177-228)
# =============================================================================

# signbit for Integer - checks if value is negative
# Based on Julia's base/int.jl:177
function signbit(x::Int64)
    return x < 0
end

# signbit for Unsigned - always false
# Based on Julia's base/int.jl:178
function signbit(x::UInt64)
    return false
end

# flipsign for Signed integers
# Based on Julia's base/int.jl:182-188
# flipsign(x, y) returns x with sign flipped if y is negative
function flipsign(x::Int64, y::Int64)
    if signbit(y)
        return -x
    else
        return x
    end
end

# abs for Unsigned - always returns itself
# Based on Julia's base/int.jl:227
function abs(x::UInt64)
    return x
end

# abs for Signed - uses flipsign
# Based on Julia's base/int.jl:228
function abs(x::Int64)
    return flipsign(x, x)
end

# Greatest common divisor (Int64 specialized)
# Returns Int64 to enable correct dispatch for div(num, gcd(...))
# Note: A builtin gcd also exists for BigInt support (builtins_math.rs)
function gcd(a::Int64, b::Int64)
    a = abs(a)
    b = abs(b)
    while b != 0
        t = b
        # Use rem (%) for modulo which returns Int64 for Int64 args
        b = a % b
        a = t
    end
    return a
end

# GCD for smaller integer types (promote to Int64 for computation)
function gcd(a::Int32, b::Int32)
    return Int32(gcd(Int64(a), Int64(b)))
end

function gcd(a::Int16, b::Int16)
    return Int16(gcd(Int64(a), Int64(b)))
end

function gcd(a::Int8, b::Int8)
    return Int8(gcd(Int64(a), Int64(b)))
end

# Integer division for Int64 - returns Int64 (not Float64)
# This ensures div(num, g) inside Rational constructor returns Int64
# IMPORTANT: Cannot use ÷ here because it's lowered to div() causing infinite recursion
# Uses sdiv_int intrinsic directly (matches Julia's checked_sdiv_int)
function div(x::Int64, y::Int64)
    # sdiv_int is the low-level intrinsic - does not call div()
    return sdiv_int(x, y)
end

# Integer division for smaller integer types (promote to Int64)
function div(x::Int32, y::Int32)
    return Int32(sdiv_int(Int64(x), Int64(y)))
end

function div(x::Int16, y::Int16)
    return Int16(sdiv_int(Int64(x), Int64(y)))
end

function div(x::Int8, y::Int8)
    return Int8(sdiv_int(Int64(x), Int64(y)))
end

# =============================================================================
# Parity functions (based on Julia's base/int.jl:154-175)
# =============================================================================

# isodd: return true if x is odd
function isodd(n::Int64)
    return (n % 2) != 0
end

function isodd(n::UInt64)
    return (n % 2) != 0
end

# iseven: return true if x is even
function iseven(n::Int64)
    return (n % 2) == 0
end

function iseven(n::UInt64)
    return (n % 2) == 0
end

# =============================================================================
# Type bounds (based on Julia's base/int.jl:849-864)
# =============================================================================

# typemax: highest value representable by a numeric type
function typemax(::Type{Int64})
    return 9223372036854775807
end

function typemax(::Type{UInt64})
    return 0xffffffffffffffff
end

# typemin: lowest value representable by a numeric type
# Note: Int64 minimum is computed as 0 - typemax(Int64) - 1 to avoid literal parsing issues
function typemin(::Type{Int64})
    return 0 - 9223372036854775807 - 1
end

function typemin(::Type{UInt64})
    return UInt64(0)
end

# typemax/typemin for UInt8, UInt16, UInt32 (Issue #3143)
function typemax(::Type{UInt32})
    return 4294967295
end

function typemax(::Type{UInt16})
    return 65535
end

function typemax(::Type{UInt8})
    return 255
end

function typemin(::Type{UInt32})
    return UInt32(0)
end

function typemin(::Type{UInt16})
    return UInt16(0)
end

function typemin(::Type{UInt8})
    return UInt8(0)
end

# =============================================================================
# Division with remainder (based on Julia's base/div.jl:196-213)
# =============================================================================

# divrem: quotient and remainder from Euclidean division
function divrem(x::Int64, y::Int64)
    return (div(x, y), x % y)
end

function divrem(x::UInt64, y::UInt64)
    return (div(x, y), x % y)
end

# =============================================================================
# Int64 Arithmetic Operators (using intrinsics)
# =============================================================================
# These concrete-type methods are required to prevent infinite recursion
# when Number fallback operators call promote(x, y):
#   +(x::Number, y::Number) -> promote(1, 2) -> (1, 2) -> +(1, 2)
# Without these, +(1, 2) would redispatch to the Number fallback forever.
#
# Specificity guarantees these always win over Number/Real fallbacks:
#   +(::Int64, ::Int64)       -> score 30 (concrete)
#   +(::Int64, ::Rational{T}) -> score 19 (parametric)
#   +(::Number, ::Number)     -> score 2  (abstract)

# Arithmetic
function Base.:(+)(x::Int64, y::Int64)
    add_int(x, y)
end

function Base.:(-)(x::Int64, y::Int64)
    sub_int(x, y)
end

function Base.:(*)(x::Int64, y::Int64)
    mul_int(x, y)
end

function Base.:(/)(x::Int64, y::Int64)
    div_float(Float64(x), Float64(y))
end

# Comparisons
function Base.:(==)(x::Int64, y::Int64)
    eq_int(x, y)
end

function Base.:(!=)(x::Int64, y::Int64)
    ne_int(x, y)
end

function Base.:(<)(x::Int64, y::Int64)
    slt_int(x, y)
end

function Base.:(<=)(x::Int64, y::Int64)
    sle_int(x, y)
end

function Base.:(>)(x::Int64, y::Int64)
    sgt_int(x, y)
end

function Base.:(>=)(x::Int64, y::Int64)
    sge_int(x, y)
end

# =============================================================================
# Bitwise Operators (using intrinsics)
# =============================================================================
# Based on Julia's base/int.jl:393, 418-419, 573-576

# Bitwise AND
function Base.:(&)(x::Int64, y::Int64)
    and_int(x, y)
end

# Bitwise OR
function Base.:(|)(x::Int64, y::Int64)
    or_int(x, y)
end

# Bitwise XOR (⊻ is an alias for xor in Julia)
function xor(x::Int64, y::Int64)
    xor_int(x, y)
end

function Base.:(⊻)(x::Int64, y::Int64)
    xor_int(x, y)
end

# Bitwise NOT
function Base.:(~)(x::Int64)
    not_int(x)
end

# =============================================================================
# Bit-Shift Operators (using intrinsics)
# =============================================================================
# Based on Julia's base/int.jl:570-585

# Left shift: a << b
function Base.:(<<)(x::Int64, y::Int64)
    shl_int(x, y)
end

# Arithmetic right shift: a >> b (preserves sign)
function Base.:(>>)(x::Int64, y::Int64)
    ashr_int(x, y)
end

# Logical right shift: a >>> b (fills with zeros)
function Base.:(>>>)(x::Int64, y::Int64)
    lshr_int(x, y)
end
