# =============================================================================
# float.jl - Floating-Point Arithmetic
# =============================================================================
# Based on Julia's base/float.jl
# Defines arithmetic and comparison operators for floating-point numbers using intrinsics.

# =============================================================================
# Floating-Point Arithmetic
# =============================================================================

# Unary minus
function Base.:(-)(x::Float64)
    neg_float(x)
end

# Binary subtraction
function Base.:(-)(x::Float64, y::Float64)
    sub_float(x, y)
end

# Binary addition
function Base.:(+)(x::Float64, y::Float64)
    add_float(x, y)
end

# Multiplication
function Base.:(*)(x::Float64, y::Float64)
    mul_float(x, y)
end

# Division
function Base.:(/)(x::Float64, y::Float64)
    div_float(x, y)
end

# Power
function Base.:(^)(x::Float64, y::Float64)
    pow_float(x, y)
end

# =============================================================================
# Mixed Float64 / Int64 arithmetic (Issue #7587)
# =============================================================================
# Upstream Julia leaves these to the generic fallback
#   +(x::Number, y::Number) = (px, py = promote(x, y); px + py)
# and lets the JIT erase the promote. This VM has no JIT, so a *first-class*
# call such as `^(::Float64, ::Int64)` — exactly what broadcasting applies per
# element for `x .^ 2` — reaches that promote() fallback and runs a chain of
# interpreted method calls per element (~8-240x slower than the same-type
# intrinsic; see Issue #7587). These concrete methods win on specificity over
# `::Number, ::Number`, convert the Int64 operand to Float64 once, and call the
# same-type intrinsic directly. Results match the promote path exactly, because
# `promote(::Float64, ::Int64)` already widens to `(Float64, Float64)`.
#
# Scope: deliberately restricted to the concrete `Int64` (the literal integer
# type, which is what `x .^ 2`, `x .+ 2`, … produce). `BigInt` must NOT be
# intercepted — `Float64 ∘ BigInt` promotes to `BigFloat`, not `Float64` — and
# `::Integer` would wrongly capture it (sjulia also has no `BitInteger` union
# usable in dispatch). Other integer widths (`Int32`, `Int128`, `Bool`, unsigned)
# stay on the correct promote() fallback; they are far rarer in numeric broadcasts
# and keeping them off these methods also preserves `Bool` strong-zero semantics.
Base.:(+)(x::Float64, y::Int64) = add_float(x, Float64(y))
Base.:(+)(x::Int64, y::Float64) = add_float(Float64(x), y)
Base.:(-)(x::Float64, y::Int64) = sub_float(x, Float64(y))
Base.:(-)(x::Int64, y::Float64) = sub_float(Float64(x), y)
Base.:(*)(x::Float64, y::Int64) = mul_float(x, Float64(y))
Base.:(*)(x::Int64, y::Float64) = mul_float(Float64(x), y)
Base.:(/)(x::Float64, y::Int64) = div_float(x, Float64(y))
Base.:(/)(x::Int64, y::Float64) = div_float(Float64(x), y)
Base.:(^)(x::Float64, y::Int64) = pow_float(x, Float64(y))
Base.:(^)(x::Int64, y::Float64) = pow_float(Float64(x), y)

# =============================================================================
# Floating-Point Comparisons
# =============================================================================

# Less than
function Base.:(<)(x::Float64, y::Float64)
    lt_float(x, y)
end

# Less or equal
function Base.:(<=)(x::Float64, y::Float64)
    le_float(x, y)
end

# Greater than
function Base.:(>)(x::Float64, y::Float64)
    gt_float(x, y)
end

# Greater or equal
function Base.:(>=)(x::Float64, y::Float64)
    ge_float(x, y)
end

# Equality
function Base.:(==)(x::Float64, y::Float64)
    eq_float(x, y)
end

# Not equal
function Base.:(!=)(x::Float64, y::Float64)
    ne_float(x, y)
end

# =============================================================================
# Exact mixed integer / float comparisons (Issue #8187, #8199)
# =============================================================================
# `==` / `!=` / `<` / `<=` / `>` / `>=` between a fixed-width integer
# (`Int8`…`Int128` / `UInt8`…`UInt128`) and a fixed IEEE float
# (`Float16`/`Float32`/`Float64`) must be value-based, NOT
# promote-the-integer-to-the-float-then-compare (rounding once |i| exceeds the
# float's exact range — 2^53 for Float64, 2^24 for Float32 — silently changed the
# answer, e.g. `9007199254740993 == 9.007199254740992e15` wrongly became `true`).
#
# This is deliberately NOT fixed with concrete Pure-Julia `==(::Int64,::Float64)`
# methods: sjulia's compile-time dispatch would coercion-match such a method to a
# `BigFloat`/`Float64` call (coercing the BigFloat operand to Int64) and break
# `big % big == 1.5`-style code. Instead the exact comparison lives in the VM:
#  * the compiler routes a statically-typed integer×float comparison through
#    `CallDynamicBinaryBoth` (compile/expr/binary/mod.rs), and
#  * the VM's `cmp_integer_to_f64` / `mixed_int_float_ordering`
#    (vm/numeric_identity.rs) perform the value-based comparison in `binary_both`
#    / `eval_numeric_binary_default` / the `isequal`/`in`/tuple-`==` paths. A
#    Float32/Float16 operand widens losslessly to f64 first, so the result is
#    precision-independent.
# BigFloat keeps its own promotion-based comparison path (a concrete value-based
# shortcut would break the coercion described above). The function-call / curried
# form (`==(a,b)`, `filter(<(x),arr)`) still routes through compile_call; tracked
# in Issue #8199's remaining notes.

# =============================================================================
# Sign-related functions for Float64 (based on Julia's base/float.jl)
# =============================================================================

# signbit for Float64 - checks if the sign bit is set
# Based on Julia's base/floatfuncs.jl:15
# Note: In Julia Base, this uses bitcast, but we use a simpler implementation
# that handles negative zero correctly by checking if 1/x is negative infinity
function signbit(x::Float64)
    # For negative zero, x < 0.0 returns false, but 1.0/x returns -Inf
    # This handles: -0.0, negative numbers, and -Inf correctly
    if x < 0.0
        return true
    elseif x == 0.0
        # Check for negative zero: 1.0/-0.0 = -Inf
        return (1.0 / x) < 0.0
    else
        return false
    end
end

# abs for Float64 - uses abs_float intrinsic
# Based on Julia's base/float.jl:698
function abs(x::Float64)
    return abs_float(x)
end

# =============================================================================
# Float32 Arithmetic (for type preservation)
# =============================================================================

# Unary minus for Float32
function Base.:(-)(x::Float32)
    Float32(neg_float(Float64(x)))
end

# Binary addition for Float32
function Base.:(+)(x::Float32, y::Float32)
    Float32(add_float(Float64(x), Float64(y)))
end

# Binary subtraction for Float32
function Base.:(-)(x::Float32, y::Float32)
    Float32(sub_float(Float64(x), Float64(y)))
end

# Multiplication for Float32
function Base.:(*)(x::Float32, y::Float32)
    Float32(mul_float(Float64(x), Float64(y)))
end

# Division for Float32
function Base.:(/)(x::Float32, y::Float32)
    Float32(div_float(Float64(x), Float64(y)))
end

# Power for Float32
function Base.:(^)(x::Float32, y::Float32)
    Float32(pow_float(Float64(x), Float64(y)))
end

# =============================================================================
# Mixed Float32 / Int64 arithmetic (Issues #1771, #7587)
# =============================================================================
# Same rationale and scope as the Float64 / Int64 block above (Int64 only; BigInt
# and the other integer widths stay on the promote() fallback). These concrete
# methods preserve Float32 (Float32 ∘ Int64 -> Float32, matching upstream
# promote_rule) and mirror the double-rounding of the same-type Float32 operators
# above. History: added in #1771 for type preservation, removed in favor of
# promote for tidiness, restored here for VM performance (Issue #7587).
Base.:(+)(x::Float32, y::Int64) = Float32(add_float(Float64(x), Float64(y)))
Base.:(+)(x::Int64, y::Float32) = Float32(add_float(Float64(x), Float64(y)))
Base.:(-)(x::Float32, y::Int64) = Float32(sub_float(Float64(x), Float64(y)))
Base.:(-)(x::Int64, y::Float32) = Float32(sub_float(Float64(x), Float64(y)))
Base.:(*)(x::Float32, y::Int64) = Float32(mul_float(Float64(x), Float64(y)))
Base.:(*)(x::Int64, y::Float32) = Float32(mul_float(Float64(x), Float64(y)))
Base.:(/)(x::Float32, y::Int64) = Float32(div_float(Float64(x), Float64(y)))
Base.:(/)(x::Int64, y::Float32) = Float32(div_float(Float64(x), Float64(y)))
Base.:(^)(x::Float32, y::Int64) = Float32(pow_float(Float64(x), Float64(y)))
Base.:(^)(x::Int64, y::Float32) = Float32(pow_float(Float64(x), Float64(y)))

# =============================================================================
# Float32 Comparisons
# =============================================================================

# Less than for Float32
function Base.:(<)(x::Float32, y::Float32)
    lt_float(Float64(x), Float64(y))
end

# Less or equal for Float32
function Base.:(<=)(x::Float32, y::Float32)
    le_float(Float64(x), Float64(y))
end

# Greater than for Float32
function Base.:(>)(x::Float32, y::Float32)
    gt_float(Float64(x), Float64(y))
end

# Greater or equal for Float32
function Base.:(>=)(x::Float32, y::Float32)
    ge_float(Float64(x), Float64(y))
end

# Equality for Float32
function Base.:(==)(x::Float32, y::Float32)
    eq_float(Float64(x), Float64(y))
end

# Not equal for Float32
function Base.:(!=)(x::Float32, y::Float32)
    ne_float(Float64(x), Float64(y))
end

# =============================================================================
# IEEE 754 float decomposition (Issue #6740)
# =============================================================================
# Public functions (exponent / significand / frexp / issubnormal / nextfloat /
# prevfloat) are pure Julia, mirroring upstream julia/base/float.jl &
# julia/base/math.jl. The only Rust boundary is `reinterpret` (raw bit access).
# Per-type IEEE bit-field helpers for Float16 / Float32 / Float64:
sign_mask(::Type{Float64})        = 0x8000_0000_0000_0000
exponent_mask(::Type{Float64})    = 0x7ff0_0000_0000_0000
exponent_one(::Type{Float64})     = 0x3ff0_0000_0000_0000
exponent_half(::Type{Float64})    = 0x3fe0_0000_0000_0000
significand_mask(::Type{Float64}) = 0x000f_ffff_ffff_ffff
sign_mask(::Type{Float32})        = 0x8000_0000
exponent_mask(::Type{Float32})    = 0x7f80_0000
exponent_one(::Type{Float32})     = 0x3f80_0000
exponent_half(::Type{Float32})    = 0x3f00_0000
significand_mask(::Type{Float32}) = 0x007f_ffff
sign_mask(::Type{Float16})        = 0x8000
exponent_mask(::Type{Float16})    = 0x7c00
exponent_one(::Type{Float16})     = 0x3c00
exponent_half(::Type{Float16})    = 0x3800
significand_mask(::Type{Float16}) = 0x03ff

significand_bits(::Type{Float64}) = 52
significand_bits(::Type{Float32}) = 23
significand_bits(::Type{Float16}) = 10
exponent_bits(::Type{Float64})    = 11
exponent_bits(::Type{Float32})    = 8
exponent_bits(::Type{Float16})    = 5
exponent_bias(::Type{Float64})    = 1023
exponent_bias(::Type{Float32})    = 127
exponent_bias(::Type{Float16})    = 15

# Matching unsigned/signed integer types (uinttype is the upstream name, kept
# underscored here to avoid clashing with any future public alias).
_float_uinttype(::Type{Float64}) = UInt64
_float_uinttype(::Type{Float32}) = UInt32
_float_uinttype(::Type{Float16}) = UInt16
_float_sinttype(::Type{Float64}) = Int64
_float_sinttype(::Type{Float32}) = Int32
_float_sinttype(::Type{Float16}) = Int16

# issubnormal(x): exponent field all-zero but significand non-zero.
function issubnormal(x::T) where {T<:AbstractFloat}
    y = reinterpret(_float_uinttype(T), x)
    (y & exponent_mask(T) == zero(y)) && (y & significand_mask(T) != zero(y))
end

# exponent(x): unbiased base-2 exponent (upstream julia/base/math.jl).
function exponent(x::T) where {T<:AbstractFloat}
    xs = reinterpret(_float_uinttype(T), x) & ~sign_mask(T)
    xs >= exponent_mask(T) && throw(DomainError(x, "Cannot be NaN or Inf."))
    k = Int(xs >> significand_bits(T))
    if k == 0  # x is subnormal
        xs == zero(xs) && throw(DomainError(x, "Cannot be ±0.0."))
        m = leading_zeros(xs) - exponent_bits(T)
        k = 1 - m
    end
    return k - exponent_bias(T)
end

# exponent(::Integer): upstream defines exponent over integers too; route through
# the float form (exponent only needs the magnitude). exponent(0) → DomainError.
exponent(x::Integer) = exponent(float(x))

# significand(x): x normalized to [1, 2) keeping its sign (upstream math.jl).
function significand(x::T) where {T<:AbstractFloat}
    xu = reinterpret(_float_uinttype(T), x)
    xs = xu & ~sign_mask(T)
    xs >= exponent_mask(T) && return x  # NaN or Inf
    if xs <= significand_mask(T)  # x is subnormal
        xs == zero(xs) && return x  # ±0
        m = unsigned(leading_zeros(xs) - exponent_bits(T))
        xs = xs << m
        xu = xs | (xu & sign_mask(T))
    end
    xu = (xu & ~exponent_mask(T)) | exponent_one(T)
    return reinterpret(T, xu)
end

# frexp(x): (significand in [0.5,1), exponent) such that x = sig * 2^exp.
function frexp(x::T) where {T<:AbstractFloat}
    xu = reinterpret(_float_uinttype(T), x)
    xs = xu & ~sign_mask(T)
    xs >= exponent_mask(T) && return (x, 0)  # NaN or Inf
    k = Int(xs >> significand_bits(T))
    if k == 0  # x is subnormal
        xs == zero(xs) && return (x, 0)  # ±0
        m = leading_zeros(xs) - exponent_bits(T)
        xs = xs << unsigned(m)
        xu = xs | (xu & sign_mask(T))
        k = 1 - m
    end
    k = k - (exponent_bias(T) - 1)
    xu = (xu & ~exponent_mask(T)) | exponent_half(T)
    return (reinterpret(T, xu), k)
end

# nextfloat/prevfloat: step the float's bit pattern by `d` ULPs, wrapping the
# sign across zero and saturating at ±Inf (upstream julia/base/float.jl). The
# `da % U` of upstream is written as `convert(U, da)` here (the `da > typemax(U)`
# branch already covers overflow), and the Bool `⊻` as `!=`, to fit the subset.
function _nextfloat(f::T, dneg::Bool, da::Integer) where {T<:AbstractFloat}
    U = _float_uinttype(T)
    fumax = exponent_mask(T)  # reinterpret(Unsigned, T(Inf))
    isnan(f) && return f
    fi = reinterpret(_float_sinttype(T), f)
    fneg = fi < 0
    fu = unsigned(fi & typemax(typeof(fi)))
    if da > typemax(U)
        fneg = dneg
        fu = fumax
    else
        du = convert(U, da)
        if fneg != dneg
            if du > fu
                fu = min(fumax, du - fu)
                fneg = !fneg
            else
                fu = fu - du
            end
        else
            if fumax - fu < du
                fu = fumax
            else
                fu = fu + du
            end
        end
    end
    if fneg
        fu = fu | sign_mask(T)
    end
    return reinterpret(T, fu)
end

nextfloat(x::T, d::Integer) where {T<:AbstractFloat} = _nextfloat(x, d < 0, abs(d))
nextfloat(x::T) where {T<:AbstractFloat} = _nextfloat(x, false, 1)
prevfloat(x::T, d::Integer) where {T<:AbstractFloat} = _nextfloat(x, d > 0, abs(d))
prevfloat(x::T) where {T<:AbstractFloat} = _nextfloat(x, true, 1)
