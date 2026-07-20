# Rational vs AbstractFloat ==/</<= must compare exactly (infinite precision),
# NOT by rounding both operands to Float64 first (Issue #9340).
# Upstream base/rational.jl: == needs a power-of-two denominator and exact
# numerator match; </<= cross-multiply the exact integer ratio of the float.

# == : lossy Float64 rounding would make these all true; exact comparison is
# false unless the float is exactly the rational.
@assert (1//3 == 0.3333333333333333) == false
@assert (1//2 == 0.5) == true
@assert (1//10 == 0.1) == false
@assert (9007199254740993//1 == 9.007199254740992e15) == false
@assert (0.3333333333333333 == 1//3) == false        # reversed operand order
@assert (0.5 == 1//2) == true
@assert (-1//3 == -0.3333333333333333) == false
@assert (1//4 == 0.25) == true

# != mirrors ==
@assert (1//3 != 0.3333333333333333) == true
@assert (0.5 != 1//2) == false

# < / <=
@assert (1//3 < 0.34) == true
@assert (0.3 < 1//3) == true
@assert (1//3 <= 0.3333333333333333) == false          # 1/3 > 0.3333333333333333
@assert (0.3333333333333333 <= 1//3) == true
@assert (0.5 <= 1//2) == true
@assert (1//2 <= 0.5) == true

# > / >=
@assert (1//2 > 0.4) == true
@assert (0.6 > 1//2) == true
@assert (1//2 >= 0.5) == true
@assert (0.5 >= 1//2) == true

# Non-finite floats
@assert (1//3 < Inf) == true
@assert (-Inf < 1//3) == true
@assert (1//3 < -Inf) == false
@assert (NaN == 1//2) == false
@assert (1//2 < NaN) == false
@assert (1//2 <= Inf) == true

println("ok")
true
