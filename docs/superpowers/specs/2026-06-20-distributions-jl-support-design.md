# Design: Distributions.jl Support in SubsetJuliaVM

**Date:** 2026-06-20  
**Issue:** [#7178](https://github.com/AtelierArith/ailujsoi/issues/7178)  
**Author:** Kimi Code (generated)  
**Status:** Draft — awaiting review

## 1. Goal

Enable `using Distributions` in SubsetJuliaVM so that iOS / web / REPL users can
construct common probability distributions, evaluate densities / probabilities,
compute basic statistics, and draw random samples — all without a JIT.

## 2. Non-Goals

- Full parity with upstream `Distributions.jl` (every distribution, every
  estimator, every plot recipe). The initial target is the subset used by
  educational samples and small interactive notebooks.
- Re-implementing the entire `StatsBase.jl` or `SpecialFunctions.jl` ecosystems.
  Only the parts actually required by the chosen distributions will be ported.
- Fitting / MLE support beyond the simplest cases (`fit(Normal, x)`,
  `fit(Bernoulli, x)`, `fit(MvNormal, x)`).

## 3. Background & Current State

`extern/Distributions.jl/` already exists as a reference clone, but the VM does
not load it. The VM only loads rewritten pure-Julia packages under
`subset_julia_vm/packages/`.

Relevant dependency status in the VM today:

| Dependency   | Status                                    |
|--------------|-------------------------------------------|
| Statistics   | ✅ Ported (`mean`, `var`, `std`, `cov`, `cor`, `median`, `quantile`) |
| Random       | ⚠️ Partial (`rand`, `randn`, `seed!`, `StableRNG`, `Xoshiro`) |
| LinearAlgebra| ⚠️ Partial (matmul, decompositions via builtins, no `Symmetric`/`Hermitian`/`Cholesky` factor object) |
| SpecialFunctions | ❌ Missing |
| StatsBase    | ❌ Missing |
| Distributions| ❌ Missing |

## 4. Design Overview

Add three new bundled packages in dependency order:

1. **SpecialFunctions** — gamma/beta/erf families required by continuous
   distributions.
2. **StatsBase** — histograms, weights, sampling, `mode`, `skewness`,
   `kurtosis`, `entropy`.
3. **Distributions** — distribution hierarchy, univariate continuous/discrete
   distributions, and a minimal multivariate distribution (`MvNormal`).

Also extend the existing `Random` stdlib to support explicit-RNG array
sampling and a `Sampler`/`Sampleable` scaffold.

```text
subset_julia_vm/packages/
├── SpecialFunctions/
│   ├── Project.toml
│   └── src/SpecialFunctions.jl
├── StatsBase/
│   ├── Project.toml
│   └── src/StatsBase.jl
└── Distributions/
    ├── Project.toml
    └── src/Distributions.jl
        # (optionally includes src/univariate/continuous.jl, etc.)
```

## 5. Detailed Design

### 5.1 SpecialFunctions

**Scope (Phase 1)**

| Function | Needed by |
|----------|-----------|
| `gamma` / `loggamma` | Gamma, Beta, Chisq, F, Student's t |
| `beta` / `lbeta` | Beta, F |
| `erf` / `erfc` | Normal cdf |
| `digamma` / `trigamma` | Beta / Dirichlet entropy, some fitters |
| `beta_inc` (incomplete beta) | Beta cdf, Binomial cdf |
| `gamma_inc` (incomplete gamma) | Gamma cdf, Chisq cdf |

**Implementation**

- Pure-Julia polynomial / continued-fraction approximations, matching the
  public API shape of upstream `SpecialFunctions.jl`.
- For functions not available in pure Julia within our accuracy budget, wrap
  the Rust `special` crate or `libm` via a new VM builtin category.
- Add fixture tests comparing against Julia's `SpecialFunctions` to a relative
  tolerance of ~1e-10.

### 5.2 StatsBase

**Scope (Phase 1)**

| Type / Function | Needed by |
|-----------------|-----------|
| `Weights`, `FrequencyWeights`, `ProbabilityWeights` | weighted sampling, `fit` |
| `Histogram` | empirical distributions, `fit(Histogram, x)` |
| `sample` / `sample!` | categorical / discrete sampling |
| `mode`, `modes` | discrete distribution checks |
| `skewness`, `kurtosis` | distribution statistics |
| `entropy` | `Distributions.entropy` fallback |
| `countmap` | `fit(DiscreteNonParametric, x)` |

**Implementation**

- Keep the public API surface small and aligned with upstream names so that
  upstream `Distributions.jl` code can be ported with minimal renames.
- Add only the methods used by Distributions.jl in Phase 1–4; defer
  `AbstractWeights` broadcasting and advanced sampling to later work.

### 5.3 Random Extensions

**Scope (Phase 1)**

- `rand(rng, dims...)` and `randn(rng, dims...)` compiler paths (VM
  instructions `RngRandArrayF64` / `RngRandnArrayF64` already exist but are not
  emitted).
- Introduce `Sampler` / `Sampleable` abstract types as no-op placeholders so
  that distribution code can declare `sampler(d)` without failing.
- Keep global RNG behavior unchanged.

### 5.4 Distributions

#### 5.4.1 Type Hierarchy

```julia
abstract type VariateForm end
abstract type ValueSupport end

abstract type Univariate    <: VariateForm end
abstract type Multivariate  <: VariateForm end
abstract type Matrixvariate <: VariateForm end

abstract type Discrete   <: ValueSupport end
abstract type Continuous <: ValueSupport end

abstract type Distribution{F<:VariateForm, S<:ValueSupport} end

const UnivariateDistribution          = Distribution{Univariate, S} where S
const ContinuousUnivariateDistribution = Distribution{Univariate, Continuous}
const DiscreteUnivariateDistribution   = Distribution{Univariate, Discrete}
const MultivariateDistribution         = Distribution{Multivariate, S} where S
const ContinuousMultivariateDistribution = Distribution{Multivariate, Continuous}
```

#### 5.4.2 Common API

Every univariate distribution implements:

- `params(d)` — tuple of parameters
- `mean(d)`, `var(d)`, `std(d)`, `median(d)`, `mode(d)`
- `entropy(d)` — when a closed form exists
- `pdf(d, x)` / `logpdf(d, x)`
- `cdf(d, x)` / `logcdf(d, x)`
- `quantile(d, q)` — inverse cdf (when tractable)
- `rand(rng, d)` / `rand(d)`
- `minimum(d)`, `maximum(d)`, `insupport(d, x)`

#### 5.4.3 Phase 2: Univariate Continuous Distributions

| Distribution | Sampling method | Notes |
|--------------|-----------------|-------|
| `Normal(μ, σ)` | Ziggurat / Box-Muller fallback | Reuse existing `randn` |
| `Uniform(a, b)` | `a + (b-a)*rand()` | |
| `Exponential(θ)` | `-θ * log(1-rand())` | |
| `Gamma(α, θ)` | Marsaglia-Tsang for `α≥1`, accept-reject for `α<1` | Needs `gamma`/`loggamma` |
| `Beta(α, β)` | `x/(x+y)` where `x~Gamma(α,1), y~Gamma(β,1)` | |
| `Cauchy(μ, σ)` | `μ + σ*tan(π*(rand()-0.5))` | |
| `LogNormal(μ, σ)` | `exp(μ + σ*randn())` | |
| `Weibull(α, θ)` | `θ * (-log(1-rand()))^(1/α)` | |

#### 5.4.4 Phase 3: Univariate Discrete Distributions

| Distribution | Sampling method | Notes |
|--------------|-----------------|-------|
| `Bernoulli(p)` | `rand() < p` | |
| `Binomial(n, p)` | sum of `n` Bernoulli(p) for small `n`; inverse-cdf for large `n` | Needs `beta_inc` for cdf |
| `Poisson(λ)` | Knuth for small `λ`, Ahrens-Dieter for large `λ` | |
| `Geometric(p)` | `ceil(log(1-rand()) / log(1-p))` | |
| `DiscreteUniform(a, b)` | `a + floor((b-a+1)*rand())` | |

#### 5.4.5 Phase 4: Multivariate Distributions

**MvNormal**

- `MvNormal(μ, Σ)` where `Σ` is a symmetric positive-definite matrix.
- Sampling: Cholesky decomposition `Σ = L*L'`, then `μ + L * randn(d)`.
- `pdf/logpdf`, `mean`, `cov`.
- Requires either a `Cholesky` factor object or a named-tuple return from
  `cholesky` (current VM returns `(L, U)`). Decision needed: add a minimal
  `Cholesky` wrapper type or keep `MvNormal` storing the factor as `Matrix`.

### 5.5 Fitting (Phase 5, Optional)

```julia
fit(::Type{Normal}, x)          # MLE
fit(::Type{Bernoulli}, x)       # MLE
fit(::Type{MvNormal}, x::Matrix) # MLE
fit(::Type{Histogram}, x, bins) # via StatsBase
```

## 6. Integration Steps

1. Create package directories and `Project.toml` files with correct `deps`.
2. Register each package in `subset_julia_vm/src/julia/packages/mod.rs`
   (`include_str!`, `get_bundled_package`, `get_package_include`,
   `bundled_package_names`).
3. Extend `Random` stdlib in
   `subset_julia_vm/src/julia/stdlib/Random/src/Random.jl` and update
   `stdlib/mod.rs` if new files are added.
4. Add fixture tests under `subset_julia_vm/tests/fixtures/distributions/`,
   `special_functions/`, `stats_base/` with `manifest.toml` entries.
5. Add REPL completion entries in `subset_julia_vm/src/repl/completions.rs`
   for exported distribution names.
6. Update `docs/vm/STATUS.md`, `DONE.md`, `UNIMPLEMENTED.md` per project
   conventions.

## 7. Testing Strategy

- **Reference tests:** Each fixture computes the same quantities in upstream
  Julia and compares to VM output with a small tolerance.
- **Sampling tests:** For each distribution, draw a large sample, then check
  that the empirical mean/variance is within 3 standard errors of the true
  value (seeded for determinism).
- **Edge cases:** parameter validation (`σ > 0`, `p ∈ [0,1]`, etc.) with
  precise error spans matching upstream Julia.
- **Regression:** Add fixtures to `packages/` category; prefix test names with
  `distributions_`, `special_functions_`, `stats_base_`.

## 8. Risks & Mitigations

| Risk | Mitigation |
|------|------------|
| SpecialFunction accuracy insufficient | Fall back to Rust `special` crate via VM builtins; add reference tolerances per function. |
| `MvNormal` requires Cholesky factor object | Implement minimal `LinearAlgebra.Cholesky` wrapper or store raw factor and accept small API divergence. |
| Large port effort | Strictly scope phases; deliver Phase 1–2 first and merge incrementally. |
| RNG array sampling compiler path broken | Add dedicated unit test for `rand(rng, 3, 3)` and `randn(rng, 3, 3)` before Distributions uses it. |

## 9. Success Criteria

- `using Distributions` succeeds in the REPL / iOS / web.
- `Normal(0,1)`, `Uniform(0,1)`, `Poisson(2.0)`, `Binomial(10, 0.3)`, and
  `MvNormal(zeros(2), I(2))` can be constructed, sampled, and queried for
  `pdf`/`cdf`/`mean`/`var`.
- All new code has fixture tests with upstream Julia reference values.
- `cargo nextest run --release --test fixture_tests distributions::` passes.

## 10. Open Questions

1. Should `MvNormal` store a `Cholesky` factor object or a raw matrix?
2. Which distribution is the highest priority for the first iOS sample?
3. Should `SpecialFunctions` be a `stdlib` instead of a bundled package?
