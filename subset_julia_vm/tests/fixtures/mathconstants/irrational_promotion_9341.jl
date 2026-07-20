# Issue #9341: Irrational promotion parity.
#
# Upstream base/irrationals.jl gives AbstractIrrational parametric promote_rule
# methods so `promote_type(Irrational, T)` follows T's float type: Float16 and
# Float32 keep their width, every other Real widens through Float64 (Int ->
# Float64, BigInt/BigFloat -> BigFloat). Without them sjulia's promote_rule
# fell back to typejoin === Real and operations forced Float64.
#
# This fixture also pins:
#   * BigInt arithmetic promotes to BigFloat at the active precision (the
#     `::BigInt` methods are excluded from the compile-time irrational Float64
#     fast path so they route through pure-Julia dispatch).
#   * The exact special-case values sin(π)=0.0, cos(π)=-1.0, tan(π)=0.0
#     (Float64(π) is not exactly π, so sin(Float64(π)) is 1.22e-16, not 0.0).
#   * pi + im keeps its imaginary part (no silent value corruption).
#
# Value checks compare against BigFloat(pi)/Float64(pi) etc. so they are
# precision-independent. Booleans/types verified against julia 1.12.6.
# The fixture RETURNS an aggregate boolean.

ok = true

# --- promote_type parity ---
ok = ok && (promote_type(Irrational{:π}, Float64) == Float64)
ok = ok && (promote_type(Irrational{:π}, Int64) == Float64)
ok = ok && (promote_type(Irrational{:π}, Float16) == Float16)
ok = ok && (promote_type(Irrational{:π}, Float32) == Float32)
ok = ok && (promote_type(Irrational{:ℯ}, Irrational{:π}) == Float64)
ok = ok && (promote_type(Irrational{:π}, BigInt) == BigFloat)

# --- concrete-partner arithmetic keeps the partner's float width ---
ok = ok && (typeof(pi + Float16(1)) == Float16)
ok = ok && (pi + Float16(1) == Float16(pi) + Float16(1))
ok = ok && (typeof(pi + 1.0f0) == Float32)
ok = ok && (typeof(pi + 1) == Float64)
ok = ok && (typeof(pi + 1.0) == Float64)

# --- pi + im must NOT drop the imaginary part (silent-wrong-value regression) ---
ok = ok && (pi + im == Float64(pi) + im)
ok = ok && (typeof(pi + im) == ComplexF64)
ok = ok && (imag(pi + im) == 1.0)

# --- BigInt promotes to BigFloat, both operand orders and all four ops ---
ok = ok && (typeof(pi + big(1)) == BigFloat)
ok = ok && (typeof(big(1) + pi) == BigFloat)
ok = ok && (pi + big(1) == BigFloat(pi) + BigFloat(1))
ok = ok && (typeof(pi - big(1)) == BigFloat)
ok = ok && (typeof(pi * big(2)) == BigFloat)
ok = ok && (typeof(pi / big(2)) == BigFloat)

# --- exact special-case trig values at pi ---
ok = ok && (sin(pi) == 0.0)
ok = ok && (cos(pi) == -1.0)
ok = ok && (tan(pi) == 0.0)

# --- misc identities ---
ok = ok && (2pi == 2 * Float64(pi))
ok = ok && (pi * pi == Float64(pi) * Float64(pi))
ok = ok && (Float32(pi) == Float32(3.1415927f0))
ok = ok && (pi < 4)

ok
