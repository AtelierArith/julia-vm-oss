# Issue #7265: `median(d::Distribution) = quantile(d, 0.5)` must dispatch to the
# typed Distributions method (extending the imported `Statistics.median`
# generic), not fall through to `Statistics.median(arr)` (which would call
# `length` on the distribution and raise a MethodError). `std(d::Distribution)`
# with the identical abstract annotation always worked; this guards that median
# (and the whole `d::Distribution` abstract-method family) dispatches too.
using Distributions

tol = 1e-9
ok = true

# Discrete: median is the smallest support value with cdf >= 0.5.
ok = ok && (median(Bernoulli(0.3)) == 0)
ok = ok && (median(Bernoulli(0.7)) == 1)
ok = ok && (median(DiscreteUniform(1, 6)) == 3)
ok = ok && (median(Poisson(3.0)) == 3)

# Continuous: median == location for symmetric distributions.
ok = ok && (abs(median(Normal(2.0, 3.0)) - 2.0) < tol)
ok = ok && (abs(median(Uniform(0.0, 10.0)) - 5.0) < tol)

# `std(d::Distribution) = sqrt(var(d))` keeps working (same abstract annotation).
ok = ok && (abs(std(Bernoulli(0.3)) - 0.458257569495584) < 1e-12)

# Categorical median via the same abstract `quantile(d, 0.5)` derivation.
ok = ok && (median(Categorical([0.2, 0.3, 0.5])) == 2)

ok
