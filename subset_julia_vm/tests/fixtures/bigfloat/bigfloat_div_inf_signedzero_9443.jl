# Issue #9443: BigFloat div / fld / cld / (/) over infinities and signed zeros
# must match MPFR/upstream julia. This extends bigfloat_nan_signedzero_9339
# (which fixed only cmp NaN-ordering and add/sub signed zero) to the
# division/rounded-division family:
#   * fld/cld of an infinite dividend (or a zero divisor) are NaN — upstream
#     computes them as round((x - rem(x, y, r)) / y), where rem yields NaN, not
#     as floor/ceil(x/y) which would leak a spurious ±Inf.
#   * div (truncated division) is trunc(x/y); trunc(±Inf) = ±Inf.
#   * the quotient of / and div carries the IEEE sign(x) XOR sign(y), even when
#     it underflows to a signed zero (1.0 / -Inf == -0.0).
# Verified against julia 1.12.6 (MPFR).
pinf = BigFloat(Inf); ninf = BigFloat(-Inf)
pz = zero(BigFloat); nz = -zero(BigFloat)
one_ = BigFloat("1.0"); mone = BigFloat("-1.0"); a = BigFloat("2.5")

r = Bool[]

# cld/fld of an infinite dividend are NaN (not ±Inf).
push!(r, isnan(cld(pinf, mone)))
push!(r, isnan(cld(pinf, one_)))
push!(r, isnan(cld(ninf, one_)))
push!(r, isnan(fld(pinf, one_)))
push!(r, isnan(fld(pinf, mone)))
push!(r, isnan(fld(ninf, mone)))

# cld/fld with a zero divisor are NaN.
push!(r, isnan(cld(a, pz)))
push!(r, isnan(fld(a, pz)))
push!(r, isnan(cld(a, nz)))

# cld/fld of a finite dividend by an infinite divisor.
push!(r, string(fld(one_, pinf)) == "0.0")
push!(r, isnan(cld(one_, pinf)))

# div (truncated division) over infinities: trunc(±Inf) = ±Inf, Inf/Inf = NaN.
push!(r, string(div(pinf, one_)) == "Inf")
push!(r, string(div(pinf, mone)) == "-Inf")
push!(r, string(div(ninf, one_)) == "-Inf")
push!(r, string(div(a, pz)) == "Inf")
push!(r, string(div(a, nz)) == "-Inf")
push!(r, isnan(div(pinf, pinf)))

# finite / infinite: signed-zero quotient (sign = sign(x) XOR sign(y)).
push!(r, string(one_ / ninf) == "-0.0")
push!(r, string(mone / pinf) == "-0.0")
push!(r, string(one_ / pinf) == "0.0")
push!(r, string(mone / ninf) == "0.0")
push!(r, string(a / ninf) == "-0.0")

# div signed-zero quotient.
push!(r, string(div(a, ninf)) == "-0.0")
push!(r, string(div(one_, pinf)) == "0.0")

all(r)
