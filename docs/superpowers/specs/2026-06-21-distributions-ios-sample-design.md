# iOS Application Distributions.jl Sample Design

Date: 2026-06-21

## Objective

Add a new iOS sample code entry that demonstrates `Distributions.jl` in the `SubsetJuliaVMApp` sample gallery.

## Background

- `Distributions.jl` is already a working bundled package in `subset_julia_vm/packages/Distributions/`.
- Supported API includes common univariate distributions (`Normal`, `Binomial`, etc.), `pdf`/`cdf`/`quantile`, `mean`/`var`/`std`, `rand`, and `fit`/`fit_mle`.
- Existing iOS samples follow a three-part registration pattern: `.jl` file, `samples.json` entry, Swift fallback in `CodeSamples+*.swift`.

## Design Decisions

| Item | Decision | Rationale |
|------|----------|-----------|
| Sample ID | `distributions_package` | Matches the existing package-sample naming pattern (`primes_package`, `symbolics_package`). |
| Display name | `Distributions.jl` | Consistent with `Primes.jl` and `Symbolics.jl` samples. |
| Difficulty | `Advanced` | Package samples (`Primes.jl`, `Symbolics.jl`) are placed in the advanced folder. |
| Category | `Mathematics` | Probability distributions and statistics are math-oriented. |
| Folder | `advanced` | Matches difficulty and existing package samples. |
| Swift fallback | `CodeSamples+Advanced.swift` | Same pattern as other advanced samples. |

## Proposed Approaches (Rejected)

1. **Basic distribution properties only** — Compute PDF/CDF/quantile of a single distribution. Rejected because it is less engaging and does not exercise `rand` or `fit`, which are key Distributions.jl features.
2. **Multi-distribution comparison** — Show PDF/CDF for many distributions side-by-side. Rejected because it becomes a repetitive API listing without a clear narrative.

## Selected Approach

**Sampling and fitting narrative**: define a distribution, inspect its analytical properties, draw deterministic samples, compute empirical statistics, fit a distribution to data, and briefly demonstrate a discrete distribution. This showcases the most commonly used parts of the API in one cohesive example.

## Sample Code Outline

```julia
# Distributions.jl — probability distributions, sampling, and fitting.
using Distributions
using Random

# Deterministic output for a reproducible demo.
Random.seed!(42)

# Continuous univariate distribution: Normal(μ, σ).
d = Normal(2.0, 3.0)
println("Distribution: ", d)
println("mean(d)      = ", mean(d))
println("var(d)       = ", var(d))
println("std(d)       = ", std(d))
println("median(d)    = ", median(d))
println("params(d)    = ", params(d))

# Evaluate probability functions.
x = 2.0
println("pdf(d, ", x, ")  = ", pdf(d, x))
println("cdf(d, ", x, ")  = ", cdf(d, x))
println("quantile(d, 0.95) = ", quantile(d, 0.95))

# Draw samples and compute empirical statistics.
samples = [rand(d) for _ in 1:1000]
empirical_mean = sum(samples) / length(samples)
empirical_var = sum((s - empirical_mean)^2 for s in samples) / (length(samples) - 1)
println("empirical mean ≈ ", empirical_mean)
println("empirical var  ≈ ", empirical_var)

# Fit a distribution to observed data.
data = [1.0, 2.0, 3.0, 4.0, 5.0]
fit_d = fit(Normal, data)
println("fit(Normal, data) = ", fit_d)
println("mean(fit_d)       = ", mean(fit_d))
println("std(fit_d)        = ", std(fit_d))

# Discrete distribution example.
b = Binomial(10, 0.3)
println("Binomial(10, 0.3) pmf at 3 = ", pdf(b, 3))
println("Binomial(10, 0.3) cdf at 3 = ", cdf(b, 3))
```

## Files to Modify

1. `SubsetJuliaVMApp/SubsetJuliaVMApp/Resources/Samples/advanced/distributions_package.jl` — new sample source.
2. `SubsetJuliaVMApp/SubsetJuliaVMApp/Resources/Samples/samples.json` — add JSON metadata entry.
3. `SubsetJuliaVMApp/SubsetJuliaVMApp/Models/CodeSamples+Advanced.swift` — add Swift fallback entry.
4. `SubsetJuliaVMApp/SubsetJuliaVMAppTests/SampleCodeTests.swift` — add `testDistributionsJl()` individual test.

## Verification

- Run the `.jl` file with upstream Julia to confirm output.
- Run the `.jl` file with `target/release/sjulia` to confirm sjulia compatibility.
- Run iOS sample tests via `xcodebuild` or `make test-ios-samples`.

## Assumptions

- Auto-permission mode is active; detailed clarifying questions were skipped and reasonable defaults were selected.
- The sample stays within the currently supported Distributions.jl subset (no explicit-RNG sampling, no unsupported distributions, no `LinearAlgebra` inside the sample).
