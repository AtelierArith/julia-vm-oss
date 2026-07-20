# Rational vs BigFloat ==/</<=/>/>= must compare exactly (infinite precision),
# NOT by rounding the Rational to BigFloat precision via the promote fallback
# (Issue #9424; the BigFloat sibling of Issue #9340). Upstream base/rational.jl
# cross-multiplies the exact power-of-two integer ratio of the float against
# the rational, at any BigFloat precision.

setprecision(BigFloat, 256)
x = BigFloat(1) / BigFloat(3)

# == : promote-based rounding would make these true; exact comparison is false
# because 1/3 has no finite binary representation at any precision.
@assert (1//3 == x) == false
@assert (x == 1//3) == false
@assert (1//3 != x) == true
@assert (x != 1//3) == true

# BigFloat(1)/BigFloat(3) rounds 1/3 up at 256 bits, so x > 1//3 exactly.
@assert (x < 1//3) == false
@assert (x <= 1//3) == false
@assert (x > 1//3) == true
@assert (x >= 1//3) == true
@assert (1//3 < x) == true
@assert (1//3 <= x) == true
@assert (1//3 > x) == false
@assert (1//3 >= x) == false

# Exactly representable values still compare equal at BigFloat precision.
@assert (BigFloat(0.5) == 1//2) == true
@assert (1//2 == BigFloat(0.5)) == true
@assert (BigFloat(0.25) >= 1//4) == true
@assert (BigFloat(0.25) <= 1//4) == true
@assert (BigFloat(0) == 0//1) == true
@assert (BigFloat(-0.5) == -1//2) == true

# The exactness must hold at non-default precisions too.
setprecision(BigFloat, 64)
y = BigFloat(1) / BigFloat(3)
@assert (1//3 == y) == false
@assert (y > 1//3) == true      # 1/3 rounds up at 64 bits as well
setprecision(BigFloat, 256)

# Non-finite BigFloats keep upstream semantics.
@assert (BigFloat(Inf) > 1//3) == true
@assert (1//3 < BigFloat(Inf)) == true
@assert (BigFloat(-Inf) < 1//3) == true
@assert (1//3 < BigFloat(-Inf)) == false
@assert (BigFloat(NaN) == 1//3) == false
@assert (BigFloat(NaN) < 1//3) == false
@assert (1//3 <= BigFloat(Inf)) == true

# Negative and large-magnitude rationals.
@assert (-1//3 == -x) == false
@assert (-x < -1//3) == true
@assert (BigFloat(2)^100 > (big(2)^100 - 1)//1) == true
@assert (BigFloat(2)^100 == (big(2)^100)//1) == true

println("ok")
true
