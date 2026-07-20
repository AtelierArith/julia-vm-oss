# Issue #8813: asin/acos/atan/asinh/acosh/atanh for Complex{Float64} arguments.
# Upstream julia defines these as specialized Complex methods in base/complex.jl;
# they were missing in sjulia (MethodError / type error). Expected values below
# are the upstream julia 1.12 results for z = 1.0 + 2.0im.

z = 1.0 + 2.0im

r_asin  = asin(z)
r_acos  = acos(z)
r_atan  = atan(z)
r_asinh = asinh(z)
r_acosh = acosh(z)
r_atanh = atanh(z)

e_asin  = 0.42707858639247614 + 1.5285709194809982im
e_acos  = 1.1437177404024204 - 1.5285709194809982im
e_atan  = 1.3389725222944935 + 0.40235947810852507im
e_asinh = 1.4693517443681852 + 1.063440023577752im
e_acosh = 1.5285709194809982 + 1.1437177404024204im
e_atanh = 0.1732867951399863 + 1.1780972450961724im

ok = (r_asin ≈ e_asin) &&
     (r_acos ≈ e_acos) &&
     (r_atan ≈ e_atan) &&
     (r_asinh ≈ e_asinh) &&
     (r_acosh ≈ e_acosh) &&
     (r_atanh ≈ e_atanh)

# Round-trip identities (principal branch): sin(asin(z)) == z, etc.
rt = (sin(asin(z)) ≈ z) &&
     (cos(acos(z)) ≈ z) &&
     (tan(atan(z)) ≈ z) &&
     (sinh(asinh(z)) ≈ z) &&
     (cosh(acosh(z)) ≈ z) &&
     (tanh(atanh(z)) ≈ z)

println(ok && rt)

ok && rt
