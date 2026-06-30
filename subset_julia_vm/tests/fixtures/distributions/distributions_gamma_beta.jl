using Distributions

tol = 1e-6
ok = true

# Gamma(2, 1.5) [shape, scale]
g = Gamma(2.0, 1.5)
ok = ok && (abs(mean(g) - 3.0) < tol)
ok = ok && (abs(var(g) - 4.5) < tol)
ok = ok && (abs(mode(g) - 1.5) < tol)
ok = ok && (abs(shape(g) - 2.0) < tol)
ok = ok && (abs(scale(g) - 1.5) < tol)
ok = ok && (abs(pdf(g, 3.0) - 0.18044704431548356) < tol)
ok = ok && (abs(cdf(g, 3.0) - 0.5939941502901619) < tol)
# quantile is the numeric inverse of cdf
ok = ok && (abs(cdf(g, quantile(g, 0.5)) - 0.5) < 1e-4)
ok = ok && (minimum(g) == 0.0)
ok = ok && (maximum(g) == Inf)

# Beta(2, 3)
b = Beta(2.0, 3.0)
ok = ok && (abs(mean(b) - 0.4) < tol)
ok = ok && (abs(var(b) - 0.04) < tol)
ok = ok && (abs(mode(b) - 0.3333333333333333) < tol)
ok = ok && (abs(pdf(b, 0.5) - 1.5) < tol)
ok = ok && (abs(cdf(b, 0.5) - 0.6875) < tol)
ok = ok && (abs(cdf(b, quantile(b, 0.3)) - 0.3) < 1e-4)
ok = ok && (minimum(b) == 0.0)
ok = ok && (maximum(b) == 1.0)

ok
