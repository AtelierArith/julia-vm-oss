using Distributions

tol = 1e-6
ok = true

# MvNormal with mean [1, 2] and covariance [[2, 0.5], [0.5, 1]]
d = MvNormal([1.0, 2.0], [2.0 0.5; 0.5 1.0])
ok = ok && (mean(d) == [1.0, 2.0])
ok = ok && (cov(d) == [2.0 0.5; 0.5 1.0])
ok = ok && (var(d) == [2.0, 1.0])
ok = ok && (dim(d) == 2)

# logpdf / pdf reference values (from upstream Distributions.jl)
ok = ok && (abs(logpdf(d, [1.0, 2.0]) - (-2.1176849603770567)) < tol)
ok = ok && (abs(pdf(d, [1.0, 2.0]) - 0.12030982838508356) < tol)
ok = ok && (abs(pdf(d, [0.0, 0.0]) - 0.01628216470064355) < tol)
ok = ok && (abs(logpdf(d, [2.0, 3.0]) - (-2.689113531805628)) < tol)

ok = ok && insupport(d, [0.0, 0.0])
ok = ok && !insupport(d, [0.0, 0.0, 0.0])

# Zero-mean convenience constructor: standard bivariate normal
d2 = MvNormal([1.0 0.0; 0.0 1.0])
ok = ok && (mean(d2) == [0.0, 0.0])
ok = ok && (abs(pdf(d2, [0.0, 0.0]) - 1.0 / (2.0 * pi)) < tol)
ok = ok && (abs(pdf(d2, [1.0, 0.0]) - exp(-0.5) / (2.0 * pi)) < tol)

# Diagonal covariance factorizes into independent normals
d3 = MvNormal([0.0, 0.0], [4.0 0.0; 0.0 9.0])
ok = ok && (abs(pdf(d3, [0.0, 0.0]) - 1.0 / (2.0 * pi * 6.0)) < tol)
ok = ok && (var(d3) == [4.0, 9.0])

ok
