# Issue #9450 (residual of #9443): three BigFloat-result residuals.
#   * signbit(::BigFloat) reads the sign field (a Rust primitive over the
#     astro_float backend), so a negative BigFloat zero is observable — the
#     generic `signbit(x) = x < 0` could not see it, mis-signing
#     abs/copysign/flipsign/mod of BigFloat zeros.
#   * mod(::BigFloat, ::BigFloat) mirrors upstream's AbstractFloat `mod`:
#     a zero remainder takes the divisor's sign (copysign(r, y)).
#   * generic-Real cld/fld mirror upstream base/div.jl
#     `div(x, y, r) = round((x - rem(x, y, r)) / y)` across the whole Real
#     tower (not only the BigFloat-operand methods #9443 fixed), so an
#     infinite dividend or zero divisor yields NaN instead of leaking
#     ±Inf/0.0. Same-type Float32/Float16 pairs keep upstream's
#     ceil/floor-of-widened-quotient (an infinite dividend stays ±Inf there).
# Verified against julia 1.12.6 (MPFR).

nzb = -zero(BigFloat)
pzb = zero(BigFloat)

r = Bool[]

# --- signbit(::BigFloat) observes the sign field, negative zero included ---
push!(r, signbit(nzb))
push!(r, !signbit(pzb))
push!(r, signbit(BigFloat(-1.5)))
push!(r, !signbit(BigFloat(1.5)))
push!(r, !signbit(big(NaN)))
push!(r, signbit(big(-Inf)))

# --- abs/copysign/flipsign of BigFloat signed zeros ---
push!(r, string(abs(nzb)) == "0.0")
push!(r, string(copysign(BigFloat(1.0), nzb)) == "-1.0")
push!(r, string(copysign(nzb, BigFloat(1.0))) == "0.0")
push!(r, string(flipsign(BigFloat(2.0), nzb)) == "-2.0")

# --- BigFloat mod: zero remainder takes the divisor's sign ---
push!(r, string(mod(pzb, BigFloat(-1.0))) == "-0.0")
push!(r, string(mod(nzb, BigFloat(1.0))) == "0.0")
push!(r, string(cld(nzb, BigFloat(-1.0))) == "0.0")

# --- generic-Real cld/fld: infinite dividend / zero divisor are NaN ---
push!(r, isnan(cld(Inf, 1.0)))
push!(r, isnan(fld(Inf, 1.0)))
push!(r, isnan(fld(-Inf, 1.0)))
push!(r, isnan(cld(1.0, 0.0)))
push!(r, isnan(fld(1.0, 0.0)))
push!(r, isnan(fld(-1.0, 0.0)))
push!(r, isnan(cld(big(typemax(Int128)), Float16(Inf))))
push!(r, isnan(cld(Inf, Float32(1.0))))
push!(r, isnan(cld(Inf, big(2))))

# --- infinite divisor: rem(x, y, r) can be ±Inf, like upstream ---
push!(r, isnan(fld(-1.0, Inf)))
push!(r, string(fld(1.0, Inf)) == "0.0")
push!(r, isnan(cld(1.0, Inf)))
push!(r, string(cld(-1.0, Inf)) == "0.0")

# --- same-type Float32/Float16 keep upstream ceil/floor of the quotient ---
c32 = cld(Float32(Inf), Float32(1.0))
push!(r, isinf(c32) && c32 isa Float32)
f32 = fld(Float32(Inf), Float32(1.0))
push!(r, isinf(f32) && f32 isa Float32)
c16 = cld(Float16(Inf), Float16(1.0))
push!(r, isinf(c16) && c16 isa Float16)

# --- finite quotients unchanged ---
push!(r, fld(5.5, 2.0) == 2.0)
push!(r, cld(5.5, 2.0) == 3.0)
push!(r, fld(-7.0, 3.0) == -3.0)
push!(r, cld(-7.0, 3.0) == -2.0)
push!(r, fld(7, 2.0) == 3.0)
push!(r, cld(7, 2.0) == 4.0)
push!(r, fld(big(3), 2.0) == 1.0)
push!(r, cld(Float32(5.5), Float32(2.0)) == 3.0f0)
push!(r, fld(Float32(-7.0), Float32(3.0)) == -3.0f0)

# --- mixed BigFloat×{Int64,UInt64,Int128,BigInt} operands convert exactly ---
# The runtime coercion for mixed BigFloat×integer intrinsics used to round-trip
# the integer through f64, truncating anything wider than 53 bits
# (big(typemax(Int128)) became 2^127) and corrupting the low ~75 bits of the
# result. Integers now convert exactly like MPFR/Julia.
push!(r, string(BigFloat("2.5") / big(typemax(Int128))) ==
         "1.469367938527859384960920671527807097281968114520203846511325984710821973889586e-38")
push!(r, string(BigFloat("2.5") + big(typemax(Int128))) ==
         "1.701411834604692317316873037158841057295e+38")
push!(r, string(big(typemax(Int128)) / BigFloat("2.5")) ==
         "6.805647338418769269267492148635364229079999999999999999999999999999999999999985e+37")
push!(r, string(BigFloat("1.0") * typemax(Int64)) == "9.223372036854775807e+18")
push!(r, string(BigFloat("0.5") + typemax(UInt64)) == "1.84467440737095516155e+19")

# --- signed zero of the quotient (needs the copysign(r, y) remainder rule) ---
push!(r, string(fld(0.0, -1.0)) == "-0.0")
push!(r, string(fld(-0.0, 1.0)) == "-0.0")
push!(r, string(cld(-0.0, -1.0)) == "0.0")
push!(r, string(cld(0.0, 1.0)) == "0.0")
push!(r, string(cld(0.5, -1.0)) == "-0.0")
push!(r, string(fld(0.5, -1.0)) == "-1.0")

all(r)
