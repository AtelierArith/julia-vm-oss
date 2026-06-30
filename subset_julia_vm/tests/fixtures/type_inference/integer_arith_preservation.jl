# Issue #3498: Primitive arithmetic inference must use centralized promotion
# instead of the old `F64/F32/F16/I64` cascade. Inline expressions exercise
# `infer_expr_type` directly: previously UInt8/UInt16/UInt32/UInt64 collapsed
# to Int64, and BigInt+Float widened to Int64 instead of BigFloat.
function add_big(a::BigInt, b::BigInt)
    return a + b
end

bi = add_big(BigInt(10), BigInt(20))
@assert bi == BigInt(30)
@assert typeof(bi) === BigInt

# Mixed signed integer widths (inference path)
function add_i64(a::Int64, b::Int64)
    return a + b
end

@assert add_i64(1, 2) == 3
@assert typeof(add_i64(1, 2)) === Int64

# Float arithmetic preserves Float64 (regression guard)
function add_f64(a::Float64, b::Float64)
    return a + b
end

@assert add_f64(1.0, 2.0) == 3.0
@assert typeof(add_f64(1.0, 2.0)) === Float64

# --- Issue #3498 regression cases (centralized promotion) ---------------
# Same-type unsigned arithmetic must preserve the unsigned width.
a = UInt8(1) + UInt8(2)
@assert a == 3
@assert typeof(a) === UInt8

b = UInt16(1) + UInt16(2)
@assert b == 3
@assert typeof(b) === UInt16

c = UInt32(1) + UInt32(2)
@assert c == 3
@assert typeof(c) === UInt32

d = UInt64(1) + UInt64(2)
@assert d == 3
@assert typeof(d) === UInt64

# Inline BigInt + BigInt stays BigInt (was already passing, regression guard).
e = BigInt(10) + BigInt(20)
@assert e == BigInt(30)
@assert typeof(e) === BigInt

# BigFloat dominates Float64 (Issue #3498 BigFloat short-circuit branch).
f = BigFloat(1.0) + 2.0
@assert f == 3.0
@assert typeof(f) === BigFloat

# Float32 preserved (regression guard for the float branch removal).
g = Float32(1.0) + Float32(2.0)
@assert g == Float32(3.0)
@assert typeof(g) === Float32

true
