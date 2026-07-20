# BigInt(::BigFloat) converts exactly for integer-valued BigFloats and throws
# InexactError otherwise; round(BigInt, ::BigFloat) rounds to the nearest
# integer (ties to even) first (Issue #9424). Mirrors upstream base/mpfr.jl.

setprecision(BigFloat, 256)

# Exact integer-valued conversions, including magnitudes beyond Int64/Float64.
@assert BigInt(BigFloat(0)) == big(0)
@assert BigInt(BigFloat(1)) == big(1)
@assert BigInt(BigFloat(-12345)) == big(-12345)
@assert BigInt(BigFloat(2)^100) == big(2)^100
@assert BigInt(-(BigFloat(2)^100)) == -(big(2)^100)
@assert BigInt(BigFloat(2)^100 + BigFloat(1)) == big(2)^100 + 1

# Fractional / non-finite values throw InexactError.
@assert try
    BigInt(BigFloat("1.5"))
    false
catch err
    err isa InexactError
end
@assert try
    BigInt(BigFloat(1) / BigFloat(3))
    false
catch err
    err isa InexactError
end
@assert try
    BigInt(BigFloat(Inf))
    false
catch err
    err isa InexactError
end
@assert try
    BigInt(BigFloat(NaN))
    false
catch err
    err isa InexactError
end

# round(BigInt, x): nearest integer, ties to even (default rounding mode).
@assert round(BigInt, BigFloat("2.5")) == big(2)
@assert round(BigInt, BigFloat("3.5")) == big(4)
@assert round(BigInt, BigFloat("-2.5")) == big(-2)
@assert round(BigInt, BigFloat(1) / BigFloat(3)) == big(0)
@assert round(BigInt, BigFloat(2) / BigFloat(3)) == big(1)
@assert round(BigInt, BigFloat(2)^100) == big(2)^100
@assert round(BigInt, BigFloat("1e30")) == big(10)^30

# round(BigInt, non-finite) throws InexactError.
@assert try
    round(BigInt, BigFloat(Inf))
    false
catch err
    err isa InexactError
end
@assert try
    round(BigInt, BigFloat(NaN))
    false
catch err
    err isa InexactError
end

println("ok")
true
