# Issues #9412 / #9384: mixed Irrational methods must terminate and preserve
# the wide type.
#
# #9412: Irrational × Rational ordering (<, <=, >, isless, min, max) used to
# hit the promote-fallback recursion trap (Issue #5966) and throw
# StackOverflowError, because promote(Irrational, Rational) failed to widen.
# The promote_rule work of Issue #9341 fixed the widening (both operands go to
# Float64); this fixture is the regression guard for the whole ordering
# surface, including dynamically-typed operands (function barrier), negative
# rationals, and Rational{BigInt}.
#
# #9384: BigInt × AbstractIrrational must widen to BigFloat (upstream
# promote_type(BigInt, Irrational) === BigFloat), not degrade to Float64.
# Two code paths are guarded:
#   * the direct min/max methods in base/irrationals.jl
#     (min(x::Integer, y::AbstractIrrational) lumped BigInt with machine ints)
#   * the value-level promote(::BigInt, ::AbstractIrrational) methods in
#     base/promotion.jl (reached by minmax and any promote-fallback operator)
# Arithmetic (+ - * /) already had BigInt-specific methods (Issue #9341); they
# are re-checked here so the whole mixed-method block stays consistent.
#
# All expected values verified against upstream julia 1.12 (default 256-bit
# BigFloat precision). BigFloat value checks compare against BigFloat-computed
# references rather than hard-coded digits, so they are precision-independent.
#
# The fixture RETURNS an aggregate boolean; any regression flips it to false.

q(x) = x           # function barrier: force dynamic dispatch
b() = big(1)

ok = true

# --- #9412: Irrational vs Rational ordering (static operands) ---
ok = ok && (pi < 3//4) == false
ok = ok && (3//4 < pi) == true
ok = ok && (pi <= 3//4) == false
ok = ok && (3//4 <= pi) == true
ok = ok && (pi > 3//4) == true
ok = ok && (3//4 > pi) == false
ok = ok && isless(pi, 3//4) == false
ok = ok && isless(3//4, pi) == true
ok = ok && min(pi, 3//4) == 0.75
ok = ok && max(pi, 3//4) == 3.141592653589793
ok = ok && minmax(3//4, pi) == (0.75, 3.141592653589793)

# negative rational, near-pi rationals, Rational{BigInt}
ok = ok && (pi < -3//4) == false
ok = ok && (-3//4 < pi) == true
ok = ok && (pi < 355//113) == true
ok = ok && (355//113 < pi) == false
ok = ok && (22//7 < pi) == false
ok = ok && (pi < big(3)//big(4)) == false
ok = ok && (big(3)//big(4) < pi) == true

# dynamic operands (promote fallback path, not the compile-time fast path)
ok = ok && (pi < q(3//4)) == false
ok = ok && (q(3//4) < pi) == true
ok = ok && isless(q(pi), q(3//4)) == false
ok = ok && min(pi, q(3//4)) == 0.75
ok = ok && max(pi, q(3//4)) == 3.141592653589793

# --- #9384: BigInt × AbstractIrrational widens to BigFloat ---
ok = ok && typeof(big(1) + pi) == BigFloat
ok = ok && typeof(pi + big(1)) == BigFloat
ok = ok && typeof(big(1) - pi) == BigFloat
ok = ok && typeof(big(2) * pi) == BigFloat
ok = ok && typeof(big(2) / pi) == BigFloat
ok = ok && (big(1) + pi == BigFloat(1) + BigFloat(pi))

# min/max (direct methods in irrationals.jl)
ok = ok && typeof(min(big(1), pi)) == BigFloat
ok = ok && typeof(min(pi, big(1))) == BigFloat
ok = ok && typeof(max(big(4), pi)) == BigFloat
ok = ok && typeof(max(pi, big(4))) == BigFloat
ok = ok && min(big(1), pi) == BigFloat(1)
ok = ok && max(big(4), pi) == BigFloat(4)
ok = ok && min(big(4), pi) == BigFloat(pi)
ok = ok && max(big(1), pi) == BigFloat(pi)

# promote (value-level methods in promotion.jl; also covers minmax)
p1, p2 = promote(big(1), pi)
ok = ok && typeof(p1) == BigFloat && typeof(p2) == BigFloat
ok = ok && p1 == BigFloat(1) && p2 == BigFloat(pi)
p3, p4 = promote(pi, big(1))
ok = ok && typeof(p3) == BigFloat && typeof(p4) == BigFloat
mn, mx = minmax(big(1), pi)
ok = ok && typeof(mn) == BigFloat && typeof(mx) == BigFloat
ok = ok && mn == BigFloat(1) && mx == BigFloat(pi)

# dynamic BigInt operands
ok = ok && typeof(min(b(), pi)) == BigFloat
ok = ok && typeof(max(pi, b())) == BigFloat
ok = ok && typeof(b() + pi) == BigFloat

# ordering with BigInt partners still terminates and matches upstream
ok = ok && (pi < big(1)) == false
ok = ok && (big(1) < pi) == true
ok = ok && isless(pi, big(1)) == false

println(ok)
ok
