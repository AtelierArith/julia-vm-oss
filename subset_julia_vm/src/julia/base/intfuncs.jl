# =============================================================================
# Integer Functions - Number-theoretic functions
# =============================================================================
# Based on Julia's base/intfuncs.jl

# Note: gcd(::Int64, ::Int64) is defined in int.jl

# gcd for BigInt - explicit method for better dispatch
function gcd(a::BigInt, b::BigInt)
    a = abs(a)
    b = abs(b)
    while b != big(0)
        t = b
        b = rem(a, b)
        a = t
    end
    return a
end

# Mixed types: promote to BigInt
# Note: Use BigInt() constructor instead of big() to ensure correct
# compile-time type inference for dispatch (Issue #1905, #1910)
function gcd(a::BigInt, b::Int64)
    return gcd(a, BigInt(b))
end

function gcd(a::Int64, b::BigInt)
    return gcd(BigInt(a), b)
end

# div for BigInt - integer division (truncating, returns BigInt)
# Issue #8900: BigInt `/` now returns BigFloat (matching upstream Julia).
# Cannot use `a ÷ b` here because `÷` is lowered to `div(a, b)` at the AST
# level, which would cause infinite recursion. Use the internal intrinsic
# `_bigint_idiv` that emits DivBigInt directly.
function div(a::BigInt, b::BigInt)
    return _bigint_idiv(a, b)
end

function div(a::BigInt, b::Int64)
    return div(a, BigInt(b))
end

function div(a::Int64, b::BigInt)
    return div(BigInt(a), b)
end

# lcm: least common multiple using gcd
# lcm(a, b) = |a * b| / gcd(a, b)
function lcm(a::Int64, b::Int64)
    g = gcd(a, b)
    if g == 0
        return 0
    end
    # Use ÷ for integer division (consistent with BigInt version)
    return abs((a ÷ g) * b)
end

# Generic same-type lcm for the remaining integer types (Bool, the Unsigned
# family, Int8..Int32, Int128/UInt128), mirroring upstream
# julia/base/intfuncs.jl `lcm(a::T, b::T) where T<:Integer` (Issue #8812).
# The concrete Int64/BigInt methods below stay more specific and still win.
# `checked_abs(checked_mul(...))` preserves upstream's OverflowError on
# results that exceed the operand type instead of silently wrapping.
function lcm(a::T, b::T) where {T<:Integer}
    # explicit a==0 test is to handle case of lcm(0, 0) correctly
    # explicit b==0 test is to handle case of lcm(typemin(T),0) correctly
    if a == 0 || b == 0
        return zero(a)
    else
        return checked_abs(checked_mul(a, div(b, gcd(b, a))))
    end
end

# lcm for BigInt
# Uses multiplication-based formula to avoid BigInt ÷ operator dispatch issues
function lcm(a::BigInt, b::BigInt)
    g = gcd(a, b)
    if g == big(0)
        return big(0)
    end
    # Compute quotient manually using the fact that a = q * g + 0 for a = a * b / gcd(a,b)
    # To avoid division dispatch issues, we directly return the product formula result
    # lcm(a, b) = |a| * |b| / gcd(a, b) = |a / g| * |b|
    # We can compute this using integer arithmetic: (|a| * |b|) / g
    # But that could overflow. Instead, use: |a / g * b|
    # Since g divides a exactly, a/g is exact integer division
    #
    # Note: The Rust VM's DivBigInt intrinsic handles BigInt ÷ BigInt
    # We rely on the intrinsic being called via the ÷ operator for BigInt literals
    a_abs = abs(a)
    b_abs = abs(b)
    # Use explicit BigInt literal division which is statically typed
    quotient = a_abs ÷ g
    return quotient * b_abs
end

# Mixed types: promote to BigInt
# Note: Use BigInt() constructor instead of big() (Issue #1905, #1910)
function lcm(a::BigInt, b::Int64)
    return lcm(a, BigInt(b))
end

function lcm(a::Int64, b::BigInt)
    return lcm(BigInt(a), b)
end

# Mixed signedness / mixed width pairs, mirroring upstream
# julia/base/intfuncs.jl (Issue #8812). The Unsigned×Signed pairs take the
# absolute value of the signed operand BEFORE promoting so the result stays
# non-negative in the promoted (unsigned-favoring) type. The Real×Real
# promote fallback is written in the two-variable form (not upstream's
# splat) per the anti-recursion pattern of Issue #9513, and the same-type
# `T<:Real` MethodError terminator guarantees the fallback can never
# re-dispatch onto itself when `promote` does not widen (the recursion trap
# documented for numeric binary operators, Issue #5966).
function gcd(a::Unsigned, b::Signed)
    pa, pb = promote(a, abs(b))
    return gcd(pa, pb)
end
function gcd(a::Signed, b::Unsigned)
    pa, pb = promote(abs(a), b)
    return gcd(pa, pb)
end
function lcm(a::Unsigned, b::Signed)
    pa, pb = promote(a, abs(b))
    return lcm(pa, pb)
end
function lcm(a::Signed, b::Unsigned)
    pa, pb = promote(abs(a), b)
    return lcm(pa, pb)
end
function gcd(a::Real, b::Real)
    pa, pb = promote(a, b)
    return gcd(pa, pb)
end
function lcm(a::Real, b::Real)
    pa, pb = promote(a, b)
    return lcm(pa, pb)
end
gcd(a::T, b::T) where {T<:Real} = throw(MethodError(gcd, (a, b)))
lcm(a::T, b::T) where {T<:Real} = throw(MethodError(lcm, (a, b)))

# gcd/lcm over a collection or 3+ arguments (Issue #5679). Upstream supports
# `gcd(itr)` / `gcd(a, b, c, ...)` (and the same for lcm); the empty collection
# returns the identity (gcd -> 0, lcm -> 1). Tuples are NOT a supported gcd/lcm
# collection upstream, so the collection method is restricted to AbstractArray.
function gcd(itr::AbstractArray)
    if isempty(itr)
        return zero(eltype(itr))
    end
    r = itr[1]
    for i in 2:length(itr)
        r = gcd(r, itr[i])
    end
    return r
end

function gcd(a, b, c, rest...)
    r = gcd(gcd(a, b), c)
    for x in rest
        r = gcd(r, x)
    end
    return r
end

function lcm(itr::AbstractArray)
    if isempty(itr)
        return one(eltype(itr))
    end
    r = itr[1]
    for i in 2:length(itr)
        r = lcm(r, itr[i])
    end
    return r
end

function lcm(a, b, c, rest...)
    r = lcm(lcm(a, b), c)
    for x in rest
        r = lcm(r, x)
    end
    return r
end

# div/rem with an explicit RoundingMode (Issue #5691): div(x, y, r) rounds the
# quotient according to `r`, and rem(x, y, r) = x - y * div(x, y, r). Upstream
# `RoundUp` == cld, `RoundDown` == fld, `RoundToZero` == div, `RoundFromZero`
# rounds away from zero, and `RoundNearest` rounds the quotient half-to-even.
# sjulia's `RoundingMode` is a non-parametric struct, so dispatch on its `.mode`.
function div(x, y, r::RoundingMode)
    if r.mode == :Up
        return cld(x, y)
    elseif r.mode == :Down
        return fld(x, y)
    elseif r.mode == :ToZero
        return div(x, y)
    elseif r.mode == :FromZero
        return (x * y >= 0) ? cld(x, y) : fld(x, y)
    elseif r.mode == :Nearest
        q = div(x, y)
        rr = x - q * y
        twice = 2 * rr
        if abs(twice) > abs(y) || (abs(twice) == abs(y) && isodd(q))
            q = q + (((rr > 0) == (y > 0)) ? 1 : -1)
        end
        return q
    else
        error("div: RoundingMode :$(r.mode) is not supported")
    end
end

function rem(x, y, r::RoundingMode)
    return x - y * div(x, y, r)
end

# factorial: n! = 1 * 2 * ... * n
# Mirrors upstream base/combinatorics.jl `factorial_lookup`: 0..20 fit in Int64,
# n < 0 throws DomainError, and n > 20 overflows Int64 so it throws OverflowError
# (matching upstream) instead of silently wrapping to a negative value. Use
# factorial(big(n)) for larger n. (Issue #9326)
function factorial(n::Int64)
    if n < 0
        throw(DomainError(n, "`n` must not be negative."))
    end
    if n > 20
        throw(OverflowError("$n is too large to look up in the table; consider using `factorial(big($n))` instead"))
    end
    result = 1
    for i in 2:n
        result = result * i
    end
    return result
end

# factorial for BigInt: arbitrary precision
function factorial(n::BigInt)
    if n < big(0)
        throw(DomainError(n, "factorial not defined for negative integers"))
    end
    result = big(1)
    i = big(2)
    while i <= n
        result = result * i
        i = i + big(1)
    end
    return result
end

# isqrt: integer square root — the largest integer m with m*m <= n, returned as
# the SAME integer type as n (upstream `isqrt(17) === 4`, not `4.0`). The float
# estimate `trunc(sqrt(n))` can be off by one near boundaries, so correct it; the
# comparisons use `div` rather than `s*s` so they never overflow (e.g. for
# `isqrt(typemax(Int64))`).
function isqrt(n)
    n < 0 && throw(DomainError(n, "isqrt is not defined for negative integers"))
    n == 0 && return n
    s = oftype(n, trunc(sqrt(n)))
    if s < one(n)
        s = one(n)
    end
    while s > div(n, s)
        s -= one(n)
    end
    while (s + one(n)) <= div(n, s + one(n))
        s += one(n)
    end
    return s
end

# powermod: compute (base^exp) % mod efficiently
function powermod(base, exp, m)
    result = 1
    base = base % m
    while exp > 0
        if exp % 2 == 1
            result = (result * base) % m
        end
        exp = exp ÷ 2
        base = (base * base) % m
    end
    return result
end

# invmod: modular inverse (a^-1 mod m) using extended Euclidean algorithm
function invmod(a, m)
    g = gcd(a, m)
    if g != 1
        return 0
    end
    return powermod(a, m - 2, m)
end

# =============================================================================
# Power of 2 functions
# =============================================================================

# ispow2: check if n is a power of 2
# Uses repeated division since bitwise ops not available
function ispow2(n)
    if n <= 0
        return false
    end
    while n > 1
        if n % 2 != 0
            return false
        end
        n = n ÷ 2
    end
    return true
end

# nextpow: smallest power of base >= x
# Returns base^k where k = ceil(log_base(x))
function nextpow(base, x)
    if x <= 0
        return 1
    end
    if x <= 1
        return 1
    end
    # Find smallest k such that base^k >= x
    power = 1
    while power < x
        power = power * base
    end
    return power
end

# prevpow: largest power of base <= x
# Returns base^k where k = floor(log_base(x))
function prevpow(base, x)
    if x < 1
        return 0
    end
    if x < base
        return 1
    end
    # Find largest k such that base^k <= x
    power = 1
    while power * base <= x
        power = power * base
    end
    return power
end

# =============================================================================
# Digit extraction functions
# =============================================================================

# digits: return array of digits (least significant first)
# Based on Julia's base/intfuncs.jl:1195
# Uses keyword argument base=10 (default decimal)
function digits(n; base=10, pad=1)
    # `pad` (Issue #5695): pad the result to at least `pad` digits with trailing
    # zeros (the number's leading zeros, since digits are least-significant first).
    if n == 0
        len = max(1, pad)
        return zeros(Int64, len)
    end
    # Extract digits with SIGNED arithmetic so a negative `n` yields negative
    # digits (`digits(-100) == [0, 0, -1]` upstream), not the digits of abs(n)
    # (Issue #5697). `rem`/`div` truncate toward zero, and the loops run while
    # `n != 0`, which works for both signs.
    count = 0
    temp = n
    while temp != 0
        count = count + 1
        temp = div(temp, base)
    end
    # Create result array (padded to at least `pad`) and fill; trailing slots
    # stay 0.
    len = max(count, pad)
    result = zeros(Int64, len)
    i = 1
    while n != 0
        result[i] = rem(n, base)
        n = div(n, base)
        i = i + 1
    end
    return result
end

# ndigits: count number of digits in base
# Based on Julia's base/intfuncs.jl:867
# Uses keyword argument base=10 (default decimal)
function ndigits(n; base=10)
    if n == 0
        return 1
    end
    if n < 0
        n = -n
    end
    count = 0
    while n > 0
        count = count + 1
        n = div(n, base)
    end
    return count
end

# Removed: count_digits(n) - use ndigits(n) instead (Julia standard)

function _string_base_digit(d)
    if d < 10
        return _int_to_char(48 + Int64(d))
    end
    return _int_to_char(87 + Int64(d))
end

function _string_unsigned_base(n::UInt128, base::Integer)
    if n == UInt128(0)
        return "0"
    end
    b = UInt128(base)
    result = ""
    while n != UInt128(0)
        # Workaround: compute the UInt128 remainder via div/mul/sub. Direct
        # rem(UInt128, UInt128) still errors for large dividends, while mixed
        # UInt128/Int rem can enter user-exported promote_rule fallbacks and
        # recurse. (Issue #9770; Issue #9333)
        q = div(n, b)
        d = n - q * b
        result = string(_string_base_digit(d)) * result
        n = q
    end
    return result
end

function _string_pad_body(body::String, pad::Integer)
    p = Int64(pad)
    while length(body) < p
        body = "0" * body
    end
    return body
end

function string(n::Integer; base::Integer=10, pad::Integer=1)
    if base < 2 || base > 36
        throw(ArgumentError("base must be between 2 and 36"))
    end
    if isa(n, Bool)
        body = _string_unsigned_base(n ? UInt128(1) : UInt128(0), base)
        return _string_pad_body(body, pad)
    end
    if n < 0
        body = _string_unsigned_base(UInt128(-n), base)
        return "-" * _string_pad_body(body, pad)
    end
    body = _string_unsigned_base(UInt128(n), base)
    return _string_pad_body(body, pad)
end

function string(b::Bool; base=nothing, pad=nothing)
    if base === nothing && pad === nothing
        return b ? "true" : "false"
    end
    base = base === nothing ? 10 : base
    pad = pad === nothing ? 1 : pad
    return string(b ? UInt8(1) : UInt8(0); base=base, pad=pad)
end

# bitstring(x): binary representation of x's bits, MSB first. Pure Julia over
# reinterpret-to-unsigned (the only Rust boundary). (Issue #6747)
_bitstring_bits(x::UInt8) = x
_bitstring_bits(x::UInt16) = x
_bitstring_bits(x::UInt32) = x
_bitstring_bits(x::UInt64) = x
_bitstring_bits(x::UInt128) = x
_bitstring_bits(x::Int8) = reinterpret(UInt8, x)
_bitstring_bits(x::Int16) = reinterpret(UInt16, x)
_bitstring_bits(x::Int32) = reinterpret(UInt32, x)
_bitstring_bits(x::Int64) = reinterpret(UInt64, x)
_bitstring_bits(x::Int128) = reinterpret(UInt128, x)
_bitstring_bits(x::Float16) = reinterpret(UInt16, x)
_bitstring_bits(x::Float32) = reinterpret(UInt32, x)
_bitstring_bits(x::Float64) = reinterpret(UInt64, x)
_bitstring_bits(x::Bool) = x ? 0x01 : 0x00

function bitstring(x)
    u = _bitstring_bits(x)
    n = sizeof(typeof(x)) * 8
    s = ""
    i = n - 1
    while i >= 0
        s = s * (((u >> i) & one(u)) == one(u) ? "1" : "0")
        i = i - 1
    end
    return s
end

# =============================================================================
# Additional integer utilities
# =============================================================================

# trailing_zeros: count trailing zeros in binary representation
# Now implemented as Rust builtin for performance (uses native CPU instruction)
# The Pure Julia version below is kept for reference but not used:
#
# function trailing_zeros(n)
#     if n == 0
#         return 0
#     end
#     count = 0
#     while n % 2 == 0
#         count = count + 1
#         n = div(n, 2)  # Use integer division
#     end
#     return count
# end

# Removed: ctz(n) - use trailing_zeros(n) instead (Julia standard)

# =============================================================================
# Combinatorics
# =============================================================================

# binomial: binomial coefficient C(n, k) = n! / (k! * (n-k)!)
# Uses multiplicative formula to avoid overflow
function binomial(n, k)
    if k < 0 || k > n
        return 0
    end
    if k == 0 || k == n
        return 1
    end
    # Use symmetry: C(n,k) = C(n, n-k)
    if k > n - k
        k = n - k
    end
    result = 1
    i = 1
    while i <= k
        result = div(result * (n - k + i), i)
        i = i + 1
    end
    return result
end

# gcdx: extended Euclidean algorithm
# Returns (gcd, x, y) such that gcd = a*x + b*y
function gcdx(a, b)
    if b == 0
        if a >= 0
            return (a, 1, 0)
        else
            return (-a, -1, 0)
        end
    end

    old_r = abs(a)
    r = abs(b)
    old_s = 1
    s = 0
    old_t = 0
    t = 1

    while r != 0
        q = div(old_r, r)
        temp_r = old_r - q * r
        old_r = r
        r = temp_r

        temp_s = old_s - q * s
        old_s = s
        s = temp_s

        temp_t = old_t - q * t
        old_t = t
        t = temp_t
    end

    # Adjust signs based on original inputs
    if a < 0
        old_s = -old_s
    end
    if b < 0
        old_t = -old_t
    end

    return (old_r, old_s, old_t)
end

# =============================================================================
# nextprod: Next integer >= n that is product of factors
# =============================================================================

# nextprod: find smallest integer >= n that is product of factors
# Simplified implementation for tuples of 2-3 factors (most common case)
function nextprod(factors, n)
    # Handle edge cases
    if n <= 0
        return 1
    end
    if n <= 1
        return 1
    end

    # For single factor, use nextpow (but handle n <= 0 case first)
    if length(factors) == 1
        if n <= 1
            return 1
        end
        return nextpow(factors[1], n)
    end

    # For 2 factors: find all combinations of powers
    if length(factors) == 2
        a, b = factors[1], factors[2]
        best = typemax(Int64)

        # Try all combinations of powers
        max_power_a = 0
        temp = 1
        while temp < n
            max_power_a = max_power_a + 1
            temp = temp * a
        end

        max_power_b = 0
        temp = 1
        while temp < n
            max_power_b = max_power_b + 1
            temp = temp * b
        end

        # Search all combinations
        for i in 0:max_power_a
            for j in 0:max_power_b
                prod = 1
                for k in 1:i
                    prod = prod * a
                end
                for k in 1:j
                    prod = prod * b
                end
                if prod >= n && prod < best
                    best = prod
                end
            end
        end

        return best
    end

    # For 3 factors (most common: (2, 3, 5))
    if length(factors) == 3
        a, b, c = factors[1], factors[2], factors[3]
        best = typemax(Int64)

        # Estimate maximum powers needed
        max_power_a = 0
        temp = 1
        while temp < n && max_power_a < 100
            max_power_a = max_power_a + 1
            temp = temp * a
        end

        max_power_b = 0
        temp = 1
        while temp < n && max_power_b < 100
            max_power_b = max_power_b + 1
            temp = temp * b
        end

        max_power_c = 0
        temp = 1
        while temp < n && max_power_c < 100
            max_power_c = max_power_c + 1
            temp = temp * c
        end

        # Search all combinations (limit to avoid overflow)
        for i in 0:max_power_a
            for j in 0:max_power_b
                for k in 0:max_power_c
                    prod = 1
                    for p in 1:i
                        prod = prod * a
                    end
                    for p in 1:j
                        prod = prod * b
                    end
                    for p in 1:k
                        prod = prod * c
                    end
                    if prod >= n && prod < best
                        best = prod
                    end
                end
            end
        end

        return best
    end

    # Fallback: use first factor's nextpow
    return nextpow(factors[1], n)
end

# =============================================================================
# clamp: Restrict a value to a specified range
# =============================================================================
# Based on Julia's base/intfuncs.jl:1444

# clamp(x, lo, hi): clamp x to be between lo and hi. Mirrors upstream
# `clamp(x::X, lo::L, hi::H) where {X,L,H}` (base/intfuncs.jl): the result is
# promoted to `promote_type(X, L, H)` so mixed-type args widen consistently, e.g.
# clamp(1, 0.5, 2.5) === 1.0 (Float64), not Int64 1.
function clamp(x::X, lo::L, hi::H) where {X,L,H}
    T = promote_type(X, L, H)
    return (x > hi) ? convert(T, hi) : (x < lo) ? convert(T, lo) : convert(T, x)
end

# clamp!(a, lo, hi): clamp all elements of array a to be between lo and hi
# Based on Julia's base/intfuncs.jl:1500
function clamp!(a::Array, lo, hi)
    for i in 1:length(a)
        ai = a[i]
        a[i] = clamp(ai, lo, hi)
    end
    return a
end

# literal_pow: `x ^ p` for a literal *negative* integer `p` is routed here by
# lowering (Issue #7233) so e.g. `2^-3 == 0.125` instead of throwing a
# DomainError. Positive literal exponents keep the fast `^` (Pow) path, and a
# non-literal exponent (`n=-3; 2^n`) is left on the `Pow` path too (it throws a
# DomainError for integer bases, matching upstream).
#
# Mirrors the observable behavior of upstream `Base.literal_pow(^, x, Val(p))`:
# an integer base widens to Float64 (`2^-3 == 0.125` / `big(2)^-3 == 0.125`),
# while other numeric types compute `inv(x)^(-p)` so the result stays
# type-stable (`(1//2)^-2 == 4//1`, `(1.0+2.0im)^-1 == inv(1.0+2.0im)`). The
# exponent is passed as a plain `Integer` (not `Val{p}`) so the value is usable
# in arithmetic without relying on type-parameter value extraction. Based on
# Julia's base/intfuncs.jl `literal_pow`.
@inline literal_pow(f, x, p::Integer) = f(x, p)
# Floats raise directly (float^p is slightly more accurate than inv-then-power).
@inline literal_pow(::typeof(^), x::AbstractFloat, p::Integer) = x^p
# Integer base, negative literal exponent: widen to Float64.
@inline literal_pow(::typeof(^), x::Integer, p::Integer) = p < 0 ? Float64(x)^p : x^p
# Other numeric types (Rational, Complex, ...): inv(x)^(-p) keeps it type-stable.
@inline literal_pow(::typeof(^), x, p::Integer) = p < 0 ? inv(x)^(-p) : x^p
