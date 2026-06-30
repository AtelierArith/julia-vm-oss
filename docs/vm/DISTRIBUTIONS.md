# Distributions.jl Support

SubsetJuliaVM bundles a pure-Julia educational subset of Distributions.jl under
`subset_julia_vm/packages/Distributions`. The goal is static, no-JIT execution
for common teaching examples rather than full package parity.

## Supported API

- Distribution hierarchy: `Distribution`, `UnivariateDistribution`,
  `ContinuousUnivariateDistribution`, `DiscreteUnivariateDistribution`,
  `MultivariateDistribution`
- Common functions: `params`, `mean`, `var`, `std`, `median`, `mode`, `modes`,
  `skewness`, `kurtosis`, `minimum`, `maximum`, `support`, `pdf`, `logpdf`,
  `cdf`, `quantile`, `cquantile`, `invlogcdf`, `invlogccdf`, `mgf`, `cf`,
  `entropy`, `loglikelihood`, `kldivergence`
- Sampling: `rand(d)`, `rand(rng, d)`, `rand(d, dims...)`,
  `rand(rng, d, dims...)`, `rand!(d, A)`, `rand!(rng, d, A)`, `sampler(d)`
- Fitting: `suffstats`, `fit_mle`, `fit`
- Truncation: `truncated(d, lower, upper)` and keyword bounds
- Plotting: `using Distributions, StatsPlots; plot(d)` for the core
  univariate distributions with StatsPlots wrappers

## Supported Distributions

Continuous univariate:

- `Normal`, `Uniform`, `Exponential`, `Gamma`, `Beta`, `Cauchy`, `LogNormal`,
  `Weibull`
- `TDist`, `Chisq`, `FDist`
- `Laplace`, `Logistic`, `Rayleigh`, `Pareto`, `Gumbel`, `Frechet`, `Levy`
- `Chi`, `Erlang`, `InverseGamma`, `InverseGaussian`, `Arcsine`,
  `TriangularDist`, `SymTriangularDist`, `Cosine`, `Semicircle`, `Kumaraswamy`

Discrete univariate:

- `Bernoulli`, `Binomial`, `Poisson`, `Geometric`, `DiscreteUniform`,
  `Categorical`
- `NegativeBinomial`, `Hypergeometric`, `BetaBinomial`
- `Skellam`, `Dirac`, `PoissonBinomial`

Multivariate:

- `MvNormal`

Wrappers:

- `Truncated`

## Validation Fixtures

The distributions category fixtures cover the support matrix above. The
milestone rollup fixture is:

```bash
JULIA_PROJECT=/tmp/sjulia_distributions_check \
  bash scripts/fixture_julia_parity.sh \
    subset_julia_vm/tests/fixtures/distributions/distributions_parity_7332.jl
```

Boolean-style distribution fixtures are exercised by the normal fixture harness:

```bash
timeout 1800 cargo nextest run --release --test fixture_tests --no-fail-fast distributions::chunk_000
```

## Known Limits

- The bundled package intentionally omits mixtures, censored distributions,
  matrix-variate distributions, and the full upstream fitting surface.
- `StatsPlots` currently implements the univariate distribution plotting recipe
  for the core wrapper set present when Issue #7262 landed; broader wrappers can
  be added incrementally.
