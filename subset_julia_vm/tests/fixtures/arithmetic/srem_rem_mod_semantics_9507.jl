# Regression: dynamic `%` (SremInt) must match Julia `rem` (truncated
# remainder, sign of the dividend) on every arm — floats and Int128 — and the
# pure-Julia `mod` derived from `%` must stay correct.
#   #9507: same_type_fast_path / mixed / F32 / F16 arms computed floor-mod
#          (`a - floor(a/b)*b`) instead of fmod, and returned NaN for an
#          infinite divisor.
#   #9520: the native Int128 SremInt arm computed floored `((a%b)+b)%b`, which
#          overflows i128 near typemax and wrongly applies floored semantics to
#          rem callers.
#
# `p`/`q` are untyped so `%`/`rem`/`mod` run through the dynamic
# CallDynamicBinaryBoth path (the statically specialized path was already
# correct). The extra statement blocks specialization.

function p(x, y)
    r = x % y
    s = r != 0
    return r
end

function q(f, x, y)
    r = f(x, y)
    s = r != 0
    return r
end

# --- #9507 bug 2: floor-vs-trunc sign for mixed / F32 / F16 pairs ---
@assert p(-7.0, 3) == -1.0                                   # mixed F64/I64
@assert p(Float32(-7.0), Float32(3.0)) == -1.0f0             # same-type F32
@assert p(-7.0, 3.0) == -1.0                                 # same-type F64
@assert p(Float16(-7.0), Float16(3.0)) == Float16(-1.0)      # same-type F16
@assert p(Float16(-7.0), 3) == Float16(-1.0)                 # F16 mixed with Int

# --- #9507 bug 1: infinite divisor must return the dividend, not NaN ---
@assert p(1.0, Inf) == 1.0
@assert q(rem, 1.0, Inf) == 1.0
@assert q(mod, -1.0, Inf) == Inf                             # mod derived from %
@assert q(mod, -1.0f0, Inf32) == Inf32

# --- #9520: Int128 rem/mod near typemax must not overflow or floor ---
@assert q(mod, Int128(9), typemax(Int128)) == Int128(9)
@assert q(rem, Int128(9), typemax(Int128)) == Int128(9)
@assert q(rem, Int128(-9), typemax(Int128)) == Int128(-9)    # trunc: sign of dividend
@assert q(mod, Int128(-9), typemax(Int128)) == typemax(Int128) - Int128(9)
@assert q(rem, typemin(Int128), Int128(-1)) == Int128(0)     # no i128::MIN % -1 panic

# --- sanity: positive operands and mod sign following the divisor ---
@assert p(7.0, 3.0) == 1.0
@assert q(mod, -7.0, 3.0) == 2.0                             # mod sign = divisor
@assert q(mod, 7.0, -3.0) == -2.0

println("srem rem/mod semantics OK")

true
