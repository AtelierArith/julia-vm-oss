using Distributions

tol = 1e-6
ok = true

# Uniform(2, 4)
u = Uniform(2.0, 4.0)
ok = ok && (abs(mean(u) - 3.0) < tol)
ok = ok && (abs(var(u) - 0.3333333333333333) < tol)
ok = ok && (abs(median(u) - 3.0) < tol)
ok = ok && (abs(pdf(u, 3.0) - 0.5) < tol)
ok = ok && (abs(pdf(u, 5.0) - 0.0) < tol)
ok = ok && (abs(cdf(u, 3.0) - 0.5) < tol)
ok = ok && (abs(cdf(u, 1.0) - 0.0) < tol)
ok = ok && (abs(cdf(u, 9.0) - 1.0) < tol)
ok = ok && (abs(quantile(u, 0.25) - 2.5) < tol)
ok = ok && (abs(entropy(u) - 0.6931471805599453) < tol)
ok = ok && (minimum(u) == 2.0)
ok = ok && (maximum(u) == 4.0)
ok = ok && insupport(u, 3.0)
ok = ok && !insupport(u, 5.0)

# Exponential(2) [scale]
e = Exponential(2.0)
ok = ok && (abs(mean(e) - 2.0) < tol)
ok = ok && (abs(var(e) - 4.0) < tol)
ok = ok && (abs(median(e) - 1.3862943611198906) < tol)
ok = ok && (abs(mode(e) - 0.0) < tol)
ok = ok && (abs(rate(e) - 0.5) < tol)
ok = ok && (abs(pdf(e, 0.0) - 0.5) < tol)
ok = ok && (abs(cdf(e, 2.0) - 0.6321205588285577) < tol)
ok = ok && (abs(quantile(e, 0.5) - 1.3862943611198906) < tol)
ok = ok && (abs(entropy(e) - 1.6931471805599452) < tol)

ok
