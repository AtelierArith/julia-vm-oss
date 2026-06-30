using Distributions

# Reference values from upstream Distributions.jl
tol = 1e-6
ok = true

d = Normal(0.0, 1.0)
ok = ok && (abs(mean(d) - 0.0) < tol)
ok = ok && (abs(var(d) - 1.0) < tol)
ok = ok && (abs(std(d) - 1.0) < tol)
ok = ok && (abs(median(d) - 0.0) < tol)
ok = ok && (abs(mode(d) - 0.0) < tol)
ok = ok && (abs(pdf(d, 0.0) - 0.3989422804014327) < tol)
ok = ok && (abs(pdf(d, 1.0) - 0.24197072451914337) < tol)
ok = ok && (abs(logpdf(d, 0.0) - (-0.9189385332046727)) < tol)
ok = ok && (abs(cdf(d, 0.0) - 0.5) < tol)
ok = ok && (abs(cdf(d, 1.0) - 0.8413447460685429) < tol)
ok = ok && (abs(cdf(d, 1.96) - 0.9750021048517795) < tol)
ok = ok && (abs(quantile(d, 0.975) - 1.959963984540054) < tol)
ok = ok && (abs(quantile(d, 0.5) - 0.0) < tol)
ok = ok && (abs(entropy(d) - 1.4189385332046727) < tol)

# Shifted/scaled Normal
d2 = Normal(2.0, 3.0)
ok = ok && (abs(mean(d2) - 2.0) < tol)
ok = ok && (abs(var(d2) - 9.0) < tol)
ok = ok && (abs(std(d2) - 3.0) < tol)
ok = ok && (abs(cdf(d2, 2.0) - 0.5) < tol)
ok = ok && (abs(pdf(d2, 2.0) - 0.1329807601338109) < tol)

# Constructors and support
ok = ok && (params(d2) == (2.0, 3.0))
ok = ok && (minimum(d) == -Inf)
ok = ok && (maximum(d) == Inf)
ok = ok && (Normal() isa Normal)
ok = ok && (Normal(5.0) isa Normal)

ok
