# Optim.jl support (MVP)

Status: **MVP implemented** (milestone "Optim.jl サポート", Issue #7432).

SubsetJuliaVM bundles an upstream-adapted, pure-Julia MVP of
[Optim.jl](https://github.com/JuliaNLSolvers/Optim.jl) (reference version 2.0.1
in the local depot; upstream master is the long-term source of truth). The MVP
targets the deterministic, no-AD / user-gradient workflows that are viable on the
no-JIT iOS runtime.

The implementation lives under `subset_julia_vm/packages/Optim/src/` and preserves
the upstream directory layout so it can be expanded toward parity without
restructuring:

```
Optim/src/
  Optim.jl                                   # module, includes, exports
  types.jl                                   # optimizer hierarchy, Options, MultivariateOptimizationResults
  api.jl                                     # query API (minimizer/minimum/iterations/converged/...)
  maximize.jl                                # maximize wrapper
  utilities/generic.jl                       # numeric helpers (_var, _sqrt, _sortperm, ...)
  univariate/types.jl                        # UnivariateOptimizationResults
  univariate/solvers/golden_section.jl       # GoldenSection
  univariate/solvers/brent.jl                # Brent
  univariate/optimize/interface.jl           # optimize(f, lower, upper; ...) promotion
  multivariate/solvers/zeroth_order/nelder_mead.jl     # NelderMead
  multivariate/solvers/first_order/gradient_descent.jl # GradientDescent
  multivariate/solvers/first_order/bfgs.jl             # BFGS (HagerZhang line search)
  multivariate/optimize/interface.jl         # optimize(f, x0, method[, options]) entry points
```

The BFGS default line search lives in the `LineSearches` dependency package
(`LineSearches/src/hagerzhang.jl`, `hagerzhang_search`), and the value/gradient
caching and central finite-difference gradient live in `NLSolversBase`.

## In scope (implemented + fixture-verified)

| Surface | Upstream file(s) adapted | Fixture |
|---------|--------------------------|---------|
| `using Optim` + dependency resolution | `Optim.jl`, `Project.toml` | `optim_loading.jl` |
| `optimize(f, lower, upper, GoldenSection())` | `univariate/solvers/golden_section.jl` | `optim_univariate_basics.jl` |
| `optimize(f, lower, upper, Brent())` (default method) | `univariate/solvers/brent.jl`, `univariate/optimize/interface.jl` | `optim_univariate_basics.jl` |
| Integer/mixed-`Real` bound promotion | `univariate/optimize/interface.jl` | `optim_univariate_basics.jl` |
| `x_lower > x_upper` precise error | `golden_section.jl` / `brent.jl` | `optim_univariate_basics.jl` |
| Result/query API: `minimizer`, `minimum`, `iterations`, `converged`, `f_calls`, `g_calls`, `x_converged`, `f_converged`, `g_converged`, `lower_bound`, `upper_bound`, `rel_tol`, `abs_tol`, `g_residual` | `api.jl`, `types.jl`, `univariate/types.jl` | `optim_result_api.jl` |
| `Options(iterations=, show_trace=, store_trace=, g_abstol=, ...)` | `types.jl` | `optim_result_api.jl` |
| `maximize` (negated-objective wrapper) + `maximizer`/`maximum` | `maximize.jl` | `optim_result_api.jl` |
| `NLSolversBase`: `NonDifferentiable`, `OnceDifferentiable`, `value`, `value!`, `value_gradient!`, `f_calls`, `g_calls` | dependency package | `optim_loading.jl`, all multivariate |
| `optimize(f, x0, NelderMead())` derivative-free | `multivariate/solvers/zeroth_order/nelder_mead.jl` | `optim_nelder_mead_mvp.jl` |
| `optimize(f, g!, x0, GradientDescent())` user-gradient first-order | `multivariate/solvers/first_order/gradient_descent.jl` | `optim_gradient_descent_mvp.jl` |
| `optimize(f, g!, x0, BFGS())` user-gradient quasi-Newton | `multivariate/solvers/first_order/bfgs.jl` | `optim_bfgs_quadratic.jl`, `optim_bfgs_rosenbrock.jl` |
| `optimize(f, x0, BFGS())` no-gradient (central finite differences, `autodiff = :finite`) | `multivariate/solvers/first_order/bfgs.jl`, `NLSolversBase` | `optim_bfgs_rosenbrock.jl` |
| `LineSearches.BackTracking` (Armijo) line search | dependency package | `optim_gradient_descent_mvp.jl` |
| `LineSearches.HagerZhang` approximate-Wolfe line search + `InitialStatic` (BFGS defaults) | `LineSearches/src/hagerzhang.jl` | `optim_bfgs_*.jl` |
| `NLSolversBase` value/gradient caching + central finite-difference gradient | dependency package | `optim_bfgs_*.jl` |

All Optim fixtures pass **identically under upstream Optim.jl 1.12.6 and sjulia**;
they use qualified `Optim.<query>` accessors because upstream does not export the
query API (sjulia additionally exports them for convenience).

Acceptance parity highlights (sjulia == upstream):

- `optimize(x->(x-2)^2, -10, 10, GoldenSection())` → minimizer `2.000000001576216`, 40 iterations, 41 f-calls.
- `optimize(x->(x-2)^2, -10, 10, Brent())` → minimizer `2.0`, minimum `0.0`, 5 iterations, 6 f-calls.
- `optimize(x->sum(abs2,x), [3.0,-1.0], NelderMead())` → 34 iterations, 70 f-calls, minimizer matches upstream to full precision.
- `optimize(f, g!, [0.0,0.0], GradientDescent())` on a convex quadratic → minimizer `[1.0, 2.0]`, minimum `0.0`.
- `optimize(quadf, quadg!, [0.0,0.0], BFGS())` → minimizer **exactly** `[1.0, 2.0]`, minimum `0.0`, **1 iteration, 3 f-calls, 3 g-calls** (exact upstream parity).
- `optimize(sumsq, sumsq_g!, [3.0,-1.0,2.0], BFGS())` → minimizer **exactly** `[0.0,0.0,0.0]`, minimum `0.0`, **1 iteration, 3 f-calls, 3 g-calls** (exact upstream parity).
- `optimize(rosenbrock, rosenbrock_g!, [0.0,0.0], BFGS())` → converges to `[1, 1]` to ~1e-9, minimum < 1e-18 (sjulia takes 16 iterations; installed upstream Optim 2.2.1 takes 21 — see "call-count parity" below).
- `optimize(rosenbrock, [0.0,0.0], BFGS())` (finite-diff) → converges to `[1, 1]` to ~1e-8, minimum < 1e-10 on both.

**BFGS call-count parity.** For one-step problems (the convex quadratics above) the
minimizer, minimum, **iteration count, and `f_calls`/`g_calls` all match upstream
Optim exactly** (1 iteration, 3/3 calls) — these are asserted exactly in the
fixtures. For multi-step problems (Rosenbrock) sjulia and upstream both reach the
minimizer/minimum within solver tolerance, but the **iteration and f/g-call counts
differ** and are deliberately NOT asserted: they depend on the line-search
internals and the floating-point reduction order of the inverse-Hessian
`dot`/`mul!` (upstream uses BLAS; the no-JIT VM uses scalar loops), and they also
drift across Optim releases. Against the installed parity gold (Optim 2.2.1 /
julia 1.12.6) the user-gradient Rosenbrock takes **21** upstream iterations (35/35
calls) versus sjulia's 16; the noisy finite-difference form reaches `[1,1]` on
both but upstream's internal `converged` flag is configuration/version dependent.
The `f_calls == g_calls` invariant holds on both. The `optim_bfgs_rosenbrock.jl`
fixture therefore asserts only minimizer/minimum tolerance + `f_calls == g_calls`,
and passes identically under upstream Optim 2.2.1 and sjulia (verified per fixture).

**BFGS port bugs/workarounds.** The faithful port surfaced three no-JIT VM bugs,
each filed and (where needed) worked around: a keyword default of `Inf` resolving
to `0` (#8078, W-40), `ceil(Int, -log2(eps(Float64)))` overflowing inside the line
search closure (#8079, W-41), and an objective variable literally named `f`
breaking the BFGS closure capture (#8080 — avoided in fixtures by using named
objectives).

## Deferred (explicitly out of MVP scope)

- Full **ADTypes/ForwardDiff/finite-difference** automatic differentiation. Only
  user-supplied gradients are honored; `ADTypes` backends are marker-type stubs.
- **Quasi-Newton** solvers other than `BFGS`: `LBFGS`, `ConjugateGradient`,
  `AcceleratedGradientDescent`, `MomentumGradientDescent`, `Adam`, `AdaMax`,
  `NGMRES`. (`BFGS` is now implemented — see "In scope" above.)
- **Second-order** solvers (`Newton`, `NewtonTrustRegion`, `KrylovTrustRegion`)
  and Hessian-heavy paths.
- **Constrained** solvers (`Fminbox`, `IPNewton`, `SAMIN`, `LBFGSB`).
- Stochastic solvers (`ParticleSwarm`, `SimulatedAnnealing`) and their randomness
  parity.
- The remaining **LineSearches** suite (`MoreThuente`, `StrongWolfe`, and the
  non-`InitialStatic` initial-step guessers). `BFGS` uses the faithful
  `HagerZhang` approximate-Wolfe line search with the `InitialStatic` step guess
  (upstream BFGS defaults). `GradientDescent` still uses `BackTracking` (Armijo)
  for the MVP; that converges to the same minimizer/minimum within tolerance for
  the supported convex problems but does not reproduce upstream's exact f/g call
  counts.
- **Trace** printing / `store_trace` / `show_trace` history and `OptimizationTrace`
  query helpers (`x_trace`, `f_trace`, `simplex_trace`, ...).
- The `MathOptInterface` extension and `SparseArrays`-specific paths.
- Exhaustive upstream test-suite parity.

## Dependency packages

Optim's upstream dependency set is bundled as either functional compatibility
packages or documented no-op stubs (Issue #7478). All resolve via the standard
`@stdlib:@packages` loader with no Optim-specific loader shortcuts:

| Package | Role in MVP |
|---------|-------------|
| `NLSolversBase` | **Functional** — objective wrappers, call counters, value/gradient **caching** (for BFGS call-count parity), and a central finite-difference gradient (`autodiff = :finite`). |
| `LineSearches` | **Functional** — `BackTracking` (Armijo, for `GradientDescent`) and `HagerZhang` (approximate-Wolfe) + `InitialStatic` (BFGS defaults). `MoreThuente`/`StrongWolfe` remain placeholder types. |
| `ADTypes` | **Stub** — AD backend marker types so `using ADTypes` resolves; no AD is wired up. |
| `NaNMath` | **Stub** — NaN-returning math names; not on any MVP path. |
| `EnumX` | **Stub** — `@enumx` no-op macro; MVP uses boolean convergence flags instead of a namespaced enum. |
| `FillArrays` | **Stub** — `Zeros`/`Ones`/`Fill` materialize dense arrays. |
| `PositiveFactorizations` | **Stub** — `Positive` marker; only needed by deferred second-order solvers. |
| `LinearAlgebra`, `Printf`, `Statistics`, `SparseArrays` | Resolved via the existing stdlib/bundled loaders. |

## Known workaround

`Optim` computes the Nelder-Mead stopping objective's square root with a
pure-arithmetic Newton iteration (`utilities/generic.jl` `_sqrt`) instead of the
builtin `sqrt`, which stack-overflows on a `Float64` produced inside a bundled
package's helper chain (Issue #8042). See `docs/vm/WORKAROUNDS.md`.

## Load-time performance (Issue #8182)

`_bfgs` (`multivariate/solvers/first_order/bfgs.jl`) carries an explicit
`::MultivariateOptimizationResults` return-type annotation. This is **not**
cosmetic: without it, compile-time return-type inference of `_bfgs`'s body
explodes (~5 s, **97 % of `using Optim`** even with the Base cache loaded). The
`phidphi` closure defined inside the BFGS loop is threaded through the deep,
mutually recursive HagerZhang line-search call tree
(`hagerzhang_search → _hz_secant2! → _hz_update! → _hz_bisect!`) and
re-specialized against the concrete closure under the loop fixpoint. Declaring
the (always-exact) return type lets `build_method_tables` skip inferring the
body, dropping `compile.build_method_tables` from ~5100 ms to ~42 ms and
`using Optim` from ~5.5 s to ~0.28 s. BFGS is loaded by *every* `using Optim`
program even when unused, so this affects the whole package, including the iOS
sample `advanced/optim_package.jl`.

The annotation is a targeted, parity-preserving mitigation; the general fix —
bounding interprocedural return-type inference for closure-threaded deep
recursive call graphs (the exception-type path already caps at `depth > 16`) —
is tracked in Issue #8182. Remove the annotation once that lands. Any future
first-order solver that reuses the `HagerZhang` line search with an in-loop
`phidphi` closure (LBFGS/CG/Newton) will need the same annotation until then.
