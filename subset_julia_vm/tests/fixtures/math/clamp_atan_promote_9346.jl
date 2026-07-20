# Issue #9346: promote-consuming Base math APIs
# clamp(x, lo, hi) must promote its args (promote_type(X,L,H)) so mixed-type
# calls widen the result, and two-arg atan(y::Real, x::Real) must float+promote
# so integer/mixed args reach the two-Float64 intrinsic instead of a MethodError.

# clamp: mixed Int/Float args widen to Float64
@assert clamp(1, 0.5, 2.5) == 1.0
@assert clamp(1, 0.5, 2.5) isa Float64
@assert clamp(1, 1.5, 2.5) == 1.5
@assert clamp(1, 1.5, 2.5) isa Float64
@assert clamp(3, 1.0, 2.0) == 2.0
@assert clamp(3, 1.0, 2.0) isa Float64

# clamp: all-Int stays Int
@assert clamp(5, 1, 10) == 5
@assert clamp(5, 1, 10) isa Int64
@assert clamp(11, 1, 10) == 10

# two-arg atan with mixed / integer args
@assert abs(atan(1, 2.0) - 0.4636476090008061) < 1e-12
@assert abs(atan(1, 2) - 0.4636476090008061) < 1e-12
@assert atan(1, 2) isa Float64
@assert abs(atan(1.0, 2.0) - 0.4636476090008061) < 1e-12

true
