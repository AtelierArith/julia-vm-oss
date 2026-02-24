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

# div for BigInt - integer division
# The generic div(x,y)=floor(x/y) returns Float64 for BigInt because floor
# doesn't handle BigInt properly. We use the / operator which correctly
# does integer division for BigInt types (Issue #1688).
function div(a::BigInt, b::BigInt)
    return a / b  # For BigInt, / performs integer division
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

# factorial: n! = 1 * 2 * ... * n
# For Int64: may overflow for n > 20
function factorial(n::Int64)
    if n < 0
        throw(DomainError(n, "factorial not defined for negative integers"))
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

# isqrt: integer square root (floor of sqrt)
function isqrt(n)
    return floor(sqrt(n))
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
function digits(n; base=10)
    if n == 0
        return [0]
    end
    if n < 0
        n = -n
    end
    # Count digits first
    count = 0
    temp = n
    while temp > 0
        count = count + 1
        temp = div(temp, base)
    end
    # Create result array and fill
    result = zeros(Int64, count)
    i = 1
    while n > 0
        result[i] = n % base
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
        q = floor(old_r / r)
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

# clamp(x, lo, hi): clamp x to be between lo and hi
function clamp(x, lo, hi)
    if x > hi
        return hi
    elseif x < lo
        return lo
    else
        return x
    end
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
