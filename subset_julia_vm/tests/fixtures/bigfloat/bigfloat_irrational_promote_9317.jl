# Issue #9317: BigFloat + Irrational must promote to BigFloat (at the active
# precision), not degrade through the Float64 promote fallback.
#
# A compile-time fast path (compile/expr/binary/mod.rs) recognizes the VM-known
# irrational singletons Irrational{:π}/Irrational{:ℯ} and, for +, -, *, /, and
# the ordering comparisons <, <=, >, >=, forces BOTH operands to Float64/Float32
# before pure-Julia method dispatch. That is only correct when each operand is a
# concrete numeric whose irrational method result is Float64/Float32 (the
# fixed-width integers Bool/Int8..Int128/UInt8..UInt128, Float32, Float64).
# BigInt is EXCLUDED (its irrational method result is BigFloat, Issue #9341).
# The gate is a WHITELIST of exactly those types: every other operand falls
# through to the pure-Julia
# `+(x::AbstractFloat, y::AbstractIrrational) = x + typeof(x)(y)` method
# (base/irrationals.jl) and the `promote(::BigFloat, ::AbstractIrrational)`
# methods (base/promotion.jl), which convert the irrational via BigFloat(pi) at
# the active precision and preserve the wider type.
#
# The earlier blacklist (bail only when a *statically* BigFloat operand was
# seen) still forced the common dynamically-typed cases onto the Float64 fast
# path, because the compiler only sees them as Any:
#   * an untyped function parameter        `f(x) = x + pi; f(BigFloat(1))`
#   * an element loaded from an Any[] slot  `Any[BigFloat(1)][1] + pi`
#   * the result of a non-specialized call  `g() = BigFloat(1); g() + pi`
# The whitelist routes all three through method dispatch → BigFloat. Float16 is
# likewise excluded (the mixed method converts pi to Float16 → Float16, not
# Float64). Ordering comparisons over a BigFloat operand promote via
# `promote(::BigFloat, ::AbstractIrrational)` and terminate (no #5966 recursion).
#
# Value checks compare against `BigFloat(1) + BigFloat(pi)` etc. rather than
# hard-coded digits, so they are precision-independent. Booleans/types verified
# against julia 1.12.6 at the default 256-bit precision.
#
# The fixture RETURNS an aggregate boolean (the harness checks the returned
# value, not @test side effects), so any regression flips the result to false.

f(x) = x + pi
g() = BigFloat(1)
h(x) = x < pi

# Diagnostic output (informational; the guard is the returned boolean below).
println("typeof(BigFloat(1) + pi) = ", typeof(BigFloat(1) + pi))
println("BigFloat(1) + pi         = ", BigFloat(1) + pi)
println("typeof(f(BigFloat(1)))   = ", typeof(f(BigFloat(1))))
println("typeof(Float16(1) + pi)  = ", typeof(Float16(1) + pi))

ok = true

# --- STATIC: + promotes to BigFloat in both operand orders (pi and ℯ) ---
ok = ok && (typeof(BigFloat(1) + pi) == BigFloat)
ok = ok && (typeof(pi + BigFloat(1)) == BigFloat)
ok = ok && (BigFloat(1) + pi == BigFloat(1) + BigFloat(pi))
ok = ok && (pi + BigFloat(1) == BigFloat(pi) + BigFloat(1))
ok = ok && (typeof(BigFloat(1) + ℯ) == BigFloat)
ok = ok && (BigFloat(1) + ℯ == BigFloat(1) + BigFloat(ℯ))

# --- STATIC: - * / ---
ok = ok && (typeof(BigFloat(1) - pi) == BigFloat)
ok = ok && (typeof(pi - BigFloat(1)) == BigFloat)
ok = ok && (BigFloat(1) - pi == BigFloat(1) - BigFloat(pi))
ok = ok && (typeof(BigFloat(2) * pi) == BigFloat)
ok = ok && (typeof(pi * BigFloat(2)) == BigFloat)
ok = ok && (BigFloat(2) * pi == BigFloat(2) * BigFloat(pi))
ok = ok && (typeof(ℯ * BigFloat(3)) == BigFloat)
ok = ok && (typeof(BigFloat(1) / pi) == BigFloat)
ok = ok && (typeof(pi / BigFloat(1)) == BigFloat)
ok = ok && (BigFloat(1) / pi == BigFloat(1) / BigFloat(pi))

# --- DYNAMIC: a BigFloat the compiler only sees as Any must still promote to
#     BigFloat, not take the Float64 fast path (the real-world regression) ---
# untyped function parameter
ok = ok && (typeof(f(BigFloat(1))) == BigFloat)
ok = ok && (f(BigFloat(1)) == BigFloat(1) + BigFloat(pi))
# element loaded from an Any[] vector
ok = ok && (typeof(Any[BigFloat(1)][1] + pi) == BigFloat)
ok = ok && (Any[BigFloat(1)][1] + pi == BigFloat(1) + BigFloat(pi))
# result of a call the compiler cannot type-specialize
ok = ok && (typeof(g() + pi) == BigFloat)
ok = ok && (g() + pi == BigFloat(1) + BigFloat(pi))
# same dynamic operand stays Float64 for a concrete Float64/Int at runtime
ok = ok && (typeof(f(2.0)) == Float64)
ok = ok && (typeof(f(2)) == Float64)

# --- Float16 promotes to Float16 (mixed method converts pi to Float16), NOT
#     the Float64 the whitelist-excluded fast path would have produced ---
ok = ok && (typeof(Float16(1) + pi) == Float16)
ok = ok && (Float16(1) + pi == Float16(1) + Float16(pi))

# --- ordering comparisons terminate (no promote-fallback recursion) and agree
#     with upstream, both statically and through an Any-typed operand ---
ok = ok && (BigFloat(1) < pi)
ok = ok && !(pi < BigFloat(1))
ok = ok && (BigFloat(4) > pi)
ok = ok && (pi <= BigFloat(4))
ok = ok && !(BigFloat(1) >= pi)
ok = ok && (h(BigFloat(1)))
ok = ok && !(h(BigFloat(4)))
ok = ok && !(pi == BigFloat(1))
ok = ok && !(BigFloat(1) == pi)

# --- BigInt promotes to BigFloat at the active precision (Issue #9341): it is
#     excluded from the whitelist so BigInt operands route through the pure-Julia
#     `+(::AbstractIrrational, ::BigInt) = BigFloat(x) + BigFloat(y)` methods
#     (base/irrationals.jl) rather than the Float64 fast path. Checked in a
#     precision-independent, parity-safe form. ---
ok = ok && (Float64(big(1)) == 1.0)
ok = ok && (Float64(big(-5)) == -5.0)
ok = ok && (typeof(pi + big(1)) == BigFloat)
ok = ok && (typeof(big(1) + pi) == BigFloat)
ok = ok && (pi + big(1) == BigFloat(pi) + BigFloat(1))
ok = ok && (typeof(pi - big(1)) == BigFloat)
ok = ok && (typeof(pi * big(2)) == BigFloat)
ok = ok && (typeof(pi / big(2)) == BigFloat)
ok = ok && (typeof(ℯ + big(1)) == BigFloat)

# --- non-excluded concrete operands keep their existing (correct) behavior ---
ok = ok && (typeof(Float32(1) + pi) == Float32)
ok = ok && (typeof(1.0 + pi) == Float64)
ok = ok && (typeof(pi + 1) == Float64)
ok = ok && (typeof(pi + ℯ) == Float64)

ok
