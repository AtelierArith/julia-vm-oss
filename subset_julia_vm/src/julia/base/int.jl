# =============================================================================
# int.jl - Integer Arithmetic
# =============================================================================
# Based on Julia's base/int.jl
# Fixed-width integer dispatch follows upstream's BitSigned / BitUnsigned union
# aliases instead of enumerating every concrete width at each method site.

# =============================================================================
# Fixed-width integer aliases (based on Julia's base/int.jl)
# =============================================================================

const BitSigned = Union{Int8, Int16, Int32, Int64, Int128}
const BitUnsigned = Union{UInt8, UInt16, UInt32, UInt64, UInt128}
const BitInteger = Union{BitSigned, BitUnsigned}

# Fixed-width public constructors are pure Julia wrappers over the VM's
# underscored conversion boundaries (Issue #8777).
Int8(x) = _to_int8(x)
Int16(x) = _to_int16(x)
Int32(x) = _to_int32(x)
Int128(x) = _to_int128(x)
UInt8(x) = _to_uint8(x)
UInt16(x) = _to_uint16(x)
UInt32(x) = _to_uint32(x)
UInt64(x) = _to_uint64(x)
UInt128(x) = _to_uint128(x)

# Unary negation for Int64 - returns Int64 (not Float64)
# This ensures -x returns Int64 when x is Int64
function Base.:-(x::Int64)
    return 0 - x
end

function Base.:-(x::Int8)
    return Int8(0) - x
end

function Base.:-(x::Int16)
    return Int16(0) - x
end

function Base.:-(x::Int32)
    return Int32(0) - x
end

function Base.:-(x::Int128)
    return Int128(0) - x
end

function Base.:-(x::UInt8)
    return UInt8(0) - x
end

function Base.:-(x::UInt16)
    return UInt16(0) - x
end

function Base.:-(x::UInt32)
    return UInt32(0) - x
end

function Base.:-(x::UInt64)
    return UInt64(0) - x
end

function Base.:-(x::UInt128)
    return UInt128(0) - x
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

# Generic same-type GCD for the remaining integer types (Bool, the Unsigned
# family, Int128/UInt128). The concrete signed methods above are more specific
# and still win for Int8..Int64/BigInt; this method only fires for integer
# types that have no dedicated method. Euclidean algorithm over the operand's
# own arithmetic preserves the element type, matching upstream (Issue #9315).
# Without this, `gcd(0x06, 0x04)` — and, via the Rational constructor,
# `true // true` / `0x01 // 0x03` — hit a MethodError / promote-fallback
# recursion instead of reducing.
function gcd(a::T, b::T) where {T<:Integer}
    a = abs(a)
    b = abs(b)
    while b != zero(T)
        t = b
        b = a % b
        a = t
    end
    return a
end

# Same-type fixed-width integer division mirrors upstream's BitSigned /
# BitUnsigned methods instead of a concrete overload per width. `sdiv_int` is
# the low-level intrinsic and never calls `div`; it may return a host-wide value
# for narrow operands, so cast back to T to preserve the operand width (Issues
# #3694 / #3696 / #3701 / #9381). The explicit typemin / -1 guard preserves
# upstream checked_sdiv_int's DivideError instead of surfacing an InexactError
# from the narrowing conversion (Issue #9429).
function div(x::T, y::T) where {T<:BitSigned}
    (y == T(-1) && x == typemin(T)) && throw(DivideError())
    return T(sdiv_int(x, y))
end

function div(x::T, y::T) where {T<:BitUnsigned}
    return T(sdiv_int(x, y))
end

# Mixed integer division follows upstream's two paths:
# - signed/unsigned pairs use unsigned magnitudes so truncation keeps the
#   upstream result signedness;
# - other mixed Integer pairs promote to a common concrete integer type before
#   calling the same-type div method.
function div(x::Signed, y::Unsigned)
    if x isa BigInt
        px, py = promote(x, y)
        return div(px, py)
    end
    return flipsign(signed(div(unsigned(abs(x)), y)), x)
end

function div(x::Unsigned, y::Signed)
    if y isa BigInt
        px, py = promote(x, y)
        return div(px, py)
    end
    return unsigned(flipsign(signed(div(x, unsigned(abs(y)))), y))
end

function div(x::Integer, y::Integer)
    if typeof(x) === typeof(y)
        T = typeof(x)
        if x isa BitSigned
            (y == T(-1) && x == typemin(T)) && throw(DivideError())
            return T(sdiv_int(x, y))
        end
        if x isa BitUnsigned
            return T(sdiv_int(x, y))
        end
        return floor(x / y)
    end
    px, py = promote(x, y)
    return div(px, py)
end

# Issue #6038 / #9381: same-width fixed-width rem/mod must preserve the operand
# width. The generic math.jl fallback routes through `%` and can widen narrow
# integer results to Int64.
function rem(x::T, y::T) where {T<:BitSigned}
    return T(srem_int(x, y))
end

function rem(x::T, y::T) where {T<:BitUnsigned}
    return T(srem_int(x, y))
end

function mod(x::T, y::T) where {T<:BitSigned}
    y == T(-1) && return T(0)
    r = rem(x, y)
    if r != T(0) && (r < T(0)) != (y < T(0))
        return r + y
    end
    return r
end

function mod(x::T, y::T) where {T<:BitUnsigned}
    return rem(x, y)
end

function fld(x::T, y::T) where {T<:BitSigned}
    q = div(x, y)
    r = rem(x, y)
    return (r != T(0) && (x < T(0)) != (y < T(0))) ? q - one(T) : q
end

function fld(x::T, y::T) where {T<:BitUnsigned}
    return div(x, y)
end

function cld(x::T, y::T) where {T<:BitSigned}
    q = div(x, y)
    r = rem(x, y)
    return (r != T(0) && (x > T(0)) == (y > T(0))) ? q + one(T) : q
end

function cld(x::T, y::T) where {T<:BitUnsigned}
    q = div(x, y)
    r = rem(x, y)
    return r != T(0) ? q + one(T) : q
end

# =============================================================================
# Mixed Signed×Unsigned rem/mod/fld/cld (Issues #9336 / #9337)
# Upstream julia/base/int.jl gives div/fld/cld/rem/mod dedicated per-operator
# signedness rules instead of naive promotion (which would try
# `convert(Unsigned, negative)` and throw InexactError):
#   - div/fld/cld/rem follow the DIVIDEND's signedness
#   - mod follows the DIVISOR's signedness
# The promoted *width* is preserved; only the signedness follows the rule.
# BigInt (which is <:Signed) has no fixed-width unsigned form, so it falls back
# to the promote path exactly like the existing div(Signed, Unsigned) methods.
# =============================================================================
function rem(x::Signed, y::Unsigned)
    if x isa BigInt
        px, py = promote(x, y)
        return rem(px, py)
    end
    return flipsign(signed(rem(unsigned(abs(x)), y)), x)
end

function rem(x::Unsigned, y::Signed)
    if y isa BigInt
        px, py = promote(x, y)
        return rem(px, py)
    end
    return rem(x, unsigned(abs(y)))
end

function mod(x::Signed, y::Unsigned)
    if x isa BigInt
        px, py = promote(x, y)
        return mod(px, py)
    end
    # `rem(x, y)` is signed with the promoted width; `unsigned` gives the same
    # width. When it is negative, add the (unsigned) divisor widened to that same
    # width so the `+` is a same-type modular add (upstream folds this via
    # `remval + (remval<0)*y`).
    remval = rem(x, y)
    u = unsigned(remval)
    return remval < zero(remval) ? u + convert(typeof(u), y) : u
end

function mod(x::Unsigned, y::Signed)
    if y isa BigInt
        px, py = promote(x, y)
        return mod(px, py)
    end
    # `rem(x, y)` is unsigned with the dividend's width; reinterpret to signed
    # (same width) then, when the divisor is negative and the remainder nonzero,
    # add the (signed) divisor widened to that same width so the `+` is a
    # same-type add.
    remval = signed(rem(x, y))
    return (!iszero(remval) && y < zero(y)) ? remval + convert(typeof(remval), y) : remval
end

# fld/cld reuse the sign-ruled div/rem: floor rounds one below trunc when the
# true quotient is negative with a nonzero remainder; ceil rounds one above
# trunc when the true quotient is positive with a nonzero remainder. A negative
# quotient occurs exactly when the signed operand is negative (the other operand
# is unsigned, hence nonnegative).
function fld(x::Signed, y::Unsigned)
    if x isa BigInt
        px, py = promote(x, y)
        return fld(px, py)
    end
    q = div(x, y)
    r = rem(x, y)
    return (r != zero(r) && x < zero(x)) ? q - one(q) : q
end

function fld(x::Unsigned, y::Signed)
    if y isa BigInt
        px, py = promote(x, y)
        return fld(px, py)
    end
    q = div(x, y)
    r = rem(x, y)
    return (r != zero(r) && y < zero(y)) ? q - one(q) : q
end

function cld(x::Signed, y::Unsigned)
    if x isa BigInt
        px, py = promote(x, y)
        return cld(px, py)
    end
    q = div(x, y)
    r = rem(x, y)
    return (r != zero(r) && x > zero(x)) ? q + one(q) : q
end

function cld(x::Unsigned, y::Signed)
    if y isa BigInt
        px, py = promote(x, y)
        return cld(px, py)
    end
    q = div(x, y)
    r = rem(x, y)
    return (r != zero(r) && y > zero(y)) ? q + one(q) : q
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
# Issue #3702: bare integer literals parse as Int64; wrap each in the
# corresponding constructor so the return type matches the type argument.
function typemax(::Type{UInt32})
    return UInt32(4294967295)
end

function typemax(::Type{UInt16})
    return UInt16(65535)
end

function typemax(::Type{UInt8})
    return UInt8(255)
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

# Issue #3702: typemax/typemin for the 128-bit integer types. Without
# these `typemax(UInt128)` / `typemax(Int128)` raise NoMethodFound.
function typemax(::Type{UInt128})
    return UInt128(0xffffffffffffffffffffffffffffffff)
end

function typemin(::Type{UInt128})
    return UInt128(0)
end

function typemax(::Type{Int128})
    return Int128(170141183460469231731687303715884105727)
end

function typemin(::Type{Int128})
    # `0 - typemax(Int128) - 1` mirrors the trick used by `typemin(Int64)` —
    # the literal -170141183460469231731687303715884105728 cannot be parsed
    # directly (overflow during parse) and the unary `-` on a 128-bit literal
    # is not currently supported by the parser.
    return Int128(0) - Int128(170141183460469231731687303715884105727) - Int128(1)
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

# Julia's base/int.jl defines `rem(x::Integer, T::Type{<:Integer})` for
# type-conversion remainder forms such as `x % Unsigned`.
function rem(x::Integer, T::Type)
    if T === Unsigned
        return unsigned(x)
    elseif T === Signed
        return signed(x)
    end
    return convert(T, x)
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

# True division of two integers always goes through Float64 — never integer
# promotion (Issue #9442). Mirrors upstream `(/)(x::BitInteger, y::BitInteger) =
# float(x) / float(y)` (julia/base/int.jl). Without this, a mixed
# signed/unsigned pair such as `Int16(-1) / UInt16(5)` falls to the generic
# `/(x::Number, y::Number) = /(promote(x, y)...)` fallback, which promotes both
# operands to the common *integer* type (UInt16) and converts `-1 -> UInt16`,
# throwing `InexactError` instead of returning `-0.2`. Both `float(x)` and
# `float(y)` widen every fixed-width integer (and `Bool`) to `Float64` first, so
# the sign is preserved. This remains on `Integer` rather than `BitInteger`
# because the current subset also routes Bool true-division through this path.
# BigInt arithmetic still uses the VM runtime and more-specific BigInt methods.
function Base.:(/)(x::Integer, y::Integer)
    float(x) / float(y)
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
# Mixed Signed×Unsigned comparisons (Issue #9336)
# Upstream julia/base/int.jl compares by a sign check plus a same-width unsigned
# compare, never promoting the pair. Naive promotion would try
# `convert(Unsigned, negative)` and throw InexactError; these methods return the
# correct Bool instead. BigInt (<:Signed) has no fixed-width unsigned form, so it
# falls back to the promote path.
# =============================================================================
function Base.:(==)(x::Signed, y::Unsigned)
    if x isa BigInt
        px, py = promote(x, y)
        return px == py
    end
    return (x >= zero(x)) & (unsigned(x) == y)
end

function Base.:(==)(x::Unsigned, y::Signed)
    if y isa BigInt
        px, py = promote(x, y)
        return px == py
    end
    return (y >= zero(y)) & (x == unsigned(y))
end

function Base.:(<)(x::Signed, y::Unsigned)
    if x isa BigInt
        px, py = promote(x, y)
        return px < py
    end
    return (x < zero(x)) | (unsigned(x) < y)
end

function Base.:(<)(x::Unsigned, y::Signed)
    if y isa BigInt
        px, py = promote(x, y)
        return px < py
    end
    return (y >= zero(y)) & (x < unsigned(y))
end

function Base.:(<=)(x::Signed, y::Unsigned)
    if x isa BigInt
        px, py = promote(x, y)
        return px <= py
    end
    return (x < zero(x)) | (unsigned(x) <= y)
end

function Base.:(<=)(x::Unsigned, y::Signed)
    if y isa BigInt
        px, py = promote(x, y)
        return px <= py
    end
    return (y >= zero(y)) & (x <= unsigned(y))
end

# >, >=, != derive from <, <=, == (upstream operators.jl: >(x,y)=y<x etc.).
Base.:(>)(x::Signed, y::Unsigned) = y < x
Base.:(>)(x::Unsigned, y::Signed) = y < x
Base.:(>=)(x::Signed, y::Unsigned) = y <= x
Base.:(>=)(x::Unsigned, y::Signed) = y <= x
Base.:(!=)(x::Signed, y::Unsigned) = !(x == y)
Base.:(!=)(x::Unsigned, y::Signed) = !(x == y)

# =============================================================================
# Bitwise Operators (using intrinsics)
# =============================================================================
# Based on Julia's base/int.jl:393, 418-419, 573-576

# ÷ as a first-class function binding (Issue #10695): upstream aliases
# `const ÷ = div` (base/operators.jl). Direct `x ÷ y` code lowers straight
# to div, but macro-expanded forms (`@show 7 ÷ 2`) and value uses (`f = ÷`)
# re-dispatch the OPERATOR NAME, which had no methods. `const ÷ = div` does
# not lower (operator assignment target), so mirror the alias as a
# forwarding method; div's own method table handles all operand types.
Base.:(÷)(x, y) = div(x, y)

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

# Bitwise binary operators for Bool (Issue #8197).
# Upstream defines these in base/bool.jl (`(&)(x::Bool, y::Bool) = and_int(x, y)`,
# `(|)(x::Bool, y::Bool) = or_int(x, y)`, `xor(x::Bool, y::Bool) = (x != y)`).
# They are co-located HERE, after the `Int64` methods above, on purpose: the
# `Int64` method must stay the FIRST-registered method of each operator so it
# remains the runtime `CallTypedDispatch` fallback for mixed-type bitwise calls
# (e.g. `0x05 & 5`), which is type-safe (`and_int` widens to `Int64`, matching
# upstream `0x05 & 5 === 5`). A `Bool` method registered first would make a
# `Bool` result slot receive a widened `Int64` at runtime → `LoadSlotBool`. See
# the longer note in base/bool.jl. `and_int`/`or_int` return a `Bool` when both
# operands are `Bool` (vm/intrinsics_exec.rs), matching upstream. `⊻` and `xor`
# are tracked as separate functions in sjulia, so both get the Bool method.
Base.:(&)(x::Bool, y::Bool) = and_int(x, y)
Base.:(|)(x::Bool, y::Bool) = or_int(x, y)
xor(x::Bool, y::Bool) = (x != y)
Base.:(⊻)(x::Bool, y::Bool) = (x != y)

# Mixed-type bitwise operators (Issue #8221).
# Upstream `base/promotion.jl` promotes mixed-integer bitwise operands to a
# common type: `&(x::Integer, y::Integer) = &(promote(x, y)...)` (likewise
# `|` / `xor`). Without these methods `0x05 & 5` (UInt8 & Int64) had no exact
# same-type method and errored `MethodError: no method matching &(::UInt8,
# ::Int64)`. These promote-then-operate fallbacks are registered AFTER the
# concrete same-type methods (above), which stay more specific, so they only
# fire for genuinely mixed pairs.
#
# Implementation note: call the intrinsic on the promoted values directly
# (`and_int(p[1], p[2])`) rather than recursing into `&(promote(x, y)...)`.
# Recursing would loop forever for `BigInt` operands — sjulia has no
# `&(BigInt, BigInt)` method, so `promote(big, int)` -> `(BigInt, BigInt)` would
# re-dispatch to this same `::Integer` method (the Issue #5966 promote-fallback
# recursion trap). The direct intrinsic form terminates; for the fixed-width
# integer types `promote` widens to a common width whose intrinsic result is
# correct and matches upstream (`0x05 & 5 === 5`).
function Base.:(&)(x::Integer, y::Integer)
    p = promote(x, y)
    return and_int(p[1], p[2])
end

function Base.:(|)(x::Integer, y::Integer)
    p = promote(x, y)
    return or_int(p[1], p[2])
end

function xor(x::Integer, y::Integer)
    p = promote(x, y)
    return xor_int(p[1], p[2])
end

function Base.:(⊻)(x::Integer, y::Integer)
    p = promote(x, y)
    return xor_int(p[1], p[2])
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

function Base.:(<<)(x::BigInt, y::Int64)
    if y < 0
        return x >> -y
    end
    return x * (BigInt(2) ^ y)
end

function Base.:(>>)(x::BigInt, y::Int64)
    if y < 0
        return x << -y
    end
    return fld(x, BigInt(2) ^ y)
end

# =============================================================================
# Bitwise/Shift operators for narrow integer widths (Issue #3565)
# =============================================================================
# Following Julia, bitwise ops on the same narrow integer type return that
# same type. The intrinsics now preserve narrow-int types when both operands
# match (see vm/intrinsics_exec.rs).

# UInt8
function Base.:(&)(x::UInt8, y::UInt8); and_int(x, y) end
function Base.:(|)(x::UInt8, y::UInt8); or_int(x, y) end
function xor(x::UInt8, y::UInt8); xor_int(x, y) end
function Base.:(⊻)(x::UInt8, y::UInt8); xor_int(x, y) end
function Base.:(~)(x::UInt8); not_int(x) end
function Base.:(<<)(x::UInt8, y::Int64); shl_int(x, y) end
function Base.:(>>)(x::UInt8, y::Int64); lshr_int(x, y) end
function Base.:(>>>)(x::UInt8, y::Int64); lshr_int(x, y) end

# UInt16
function Base.:(&)(x::UInt16, y::UInt16); and_int(x, y) end
function Base.:(|)(x::UInt16, y::UInt16); or_int(x, y) end
function xor(x::UInt16, y::UInt16); xor_int(x, y) end
function Base.:(⊻)(x::UInt16, y::UInt16); xor_int(x, y) end
function Base.:(~)(x::UInt16); not_int(x) end
function Base.:(<<)(x::UInt16, y::Int64); shl_int(x, y) end
function Base.:(>>)(x::UInt16, y::Int64); lshr_int(x, y) end
function Base.:(>>>)(x::UInt16, y::Int64); lshr_int(x, y) end

# UInt32
function Base.:(&)(x::UInt32, y::UInt32); and_int(x, y) end
function Base.:(|)(x::UInt32, y::UInt32); or_int(x, y) end
function xor(x::UInt32, y::UInt32); xor_int(x, y) end
function Base.:(⊻)(x::UInt32, y::UInt32); xor_int(x, y) end
function Base.:(~)(x::UInt32); not_int(x) end
function Base.:(<<)(x::UInt32, y::Int64); shl_int(x, y) end
function Base.:(>>)(x::UInt32, y::Int64); lshr_int(x, y) end
function Base.:(>>>)(x::UInt32, y::Int64); lshr_int(x, y) end

# UInt64
function Base.:(&)(x::UInt64, y::UInt64); and_int(x, y) end
function Base.:(|)(x::UInt64, y::UInt64); or_int(x, y) end
function xor(x::UInt64, y::UInt64); xor_int(x, y) end
function Base.:(⊻)(x::UInt64, y::UInt64); xor_int(x, y) end
function Base.:(~)(x::UInt64); not_int(x) end
function Base.:(<<)(x::UInt64, y::Int64); shl_int(x, y) end
function Base.:(>>)(x::UInt64, y::Int64); lshr_int(x, y) end
function Base.:(>>>)(x::UInt64, y::Int64); lshr_int(x, y) end

# Int8
function Base.:(&)(x::Int8, y::Int8); and_int(x, y) end
function Base.:(|)(x::Int8, y::Int8); or_int(x, y) end
function xor(x::Int8, y::Int8); xor_int(x, y) end
function Base.:(⊻)(x::Int8, y::Int8); xor_int(x, y) end
function Base.:(~)(x::Int8); not_int(x) end
function Base.:(<<)(x::Int8, y::Int64); shl_int(x, y) end
function Base.:(>>)(x::Int8, y::Int64); ashr_int(x, y) end
function Base.:(>>>)(x::Int8, y::Int64); lshr_int(x, y) end

# Int16
function Base.:(&)(x::Int16, y::Int16); and_int(x, y) end
function Base.:(|)(x::Int16, y::Int16); or_int(x, y) end
function xor(x::Int16, y::Int16); xor_int(x, y) end
function Base.:(⊻)(x::Int16, y::Int16); xor_int(x, y) end
function Base.:(~)(x::Int16); not_int(x) end
function Base.:(<<)(x::Int16, y::Int64); shl_int(x, y) end
function Base.:(>>)(x::Int16, y::Int64); ashr_int(x, y) end
function Base.:(>>>)(x::Int16, y::Int64); lshr_int(x, y) end

# Int32
function Base.:(&)(x::Int32, y::Int32); and_int(x, y) end
function Base.:(|)(x::Int32, y::Int32); or_int(x, y) end
function xor(x::Int32, y::Int32); xor_int(x, y) end
function Base.:(⊻)(x::Int32, y::Int32); xor_int(x, y) end
function Base.:(~)(x::Int32); not_int(x) end
function Base.:(<<)(x::Int32, y::Int64); shl_int(x, y) end
function Base.:(>>)(x::Int32, y::Int64); ashr_int(x, y) end
function Base.:(>>>)(x::Int32, y::Int64); lshr_int(x, y) end

# Int128
function Base.:(&)(x::Int128, y::Int128); and_int(x, y) end
function Base.:(|)(x::Int128, y::Int128); or_int(x, y) end
function xor(x::Int128, y::Int128); xor_int(x, y) end
function Base.:(⊻)(x::Int128, y::Int128); xor_int(x, y) end
function Base.:(~)(x::Int128); not_int(x) end
function Base.:(<<)(x::Int128, y::Int64); shl_int(x, y) end
function Base.:(>>)(x::Int128, y::Int64); ashr_int(x, y) end
function Base.:(>>>)(x::Int128, y::Int64); lshr_int(x, y) end

# UInt128 (Issue #3747)
# Bitwise/shift methods were missed by PR #3565 — only Int128 was added there.
# Runtime intrinsics already preserve UInt128 width; the gap was pure-Julia
# dispatch entries.
function Base.:(&)(x::UInt128, y::UInt128); and_int(x, y) end
function Base.:(|)(x::UInt128, y::UInt128); or_int(x, y) end
function xor(x::UInt128, y::UInt128); xor_int(x, y) end
function Base.:(⊻)(x::UInt128, y::UInt128); xor_int(x, y) end
function Base.:(~)(x::UInt128); not_int(x) end
function Base.:(<<)(x::UInt128, y::Int64); shl_int(x, y) end
function Base.:(>>)(x::UInt128, y::Int64); lshr_int(x, y) end
function Base.:(>>>)(x::UInt128, y::Int64); lshr_int(x, y) end

# --- Primitive bit operations (Issue #6741) ---
# The public functions are pure Julia; the actual CPU operations are the
# underscored low-level intrinsics `_ctpop_int` / `_ctlz_int` / `_cttz_int`
# (popcount / count-leading-zeros / count-trailing-zeros, returning Int) and
# the type-preserving `_bitreverse_int` / `_bswap_int`. This mirrors upstream
# `count_ones(x) = ctpop_int(x) % Int` etc. (julia/base/int.jl), keeping only
# the CPU intrinsic on the Rust side. BigInt has no method (matches upstream
# BitInteger restriction — the intrinsic rejects it).
count_ones(x::Integer)     = _ctpop_int(x)
leading_zeros(x::Integer)  = _ctlz_int(x)
trailing_zeros(x::Integer) = _cttz_int(x)
bitreverse(x::Integer)     = _bitreverse_int(x)
bswap(x::Integer)          = _bswap_int(x)

# --- Derived bit helpers (Issue #6722) ---
# Defined in pure Julia exactly as upstream (julia/base/int.jl): they wrap the
# primitive bit operations above via bitwise-not.
count_zeros(x::Integer)   = count_ones(~x)
leading_ones(x::Integer)  = leading_zeros(~x)
trailing_ones(x::Integer) = trailing_zeros(~x)

# `bitrotate(x, k)` rotates the bits of a fixed-width integer left by `k`
# (right if `k < 0`), wrapping modulo the bit width. BigInt is intentionally
# left without a method (matches upstream MethodError).
_bitrotate(x::T, k) where {T} =
    (x << ((sizeof(T) << 3 - 1) & k)) | (x >>> ((sizeof(T) << 3 - 1) & -k))
bitrotate(x::T, k::Integer) where {T<:BitInteger} = _bitrotate(x, k)
