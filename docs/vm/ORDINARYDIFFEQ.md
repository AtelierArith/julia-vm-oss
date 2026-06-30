# OrdinaryDiffEq README 可視化 MVP (Issue #7360)

sjulia の `OrdinaryDiffEq` 対応は、上流 OrdinaryDiffEq.jl / SciMLBase の full port
ではなく、OrdinaryDiffEq.jl README の代表サンプルを no-JIT VM 上で実行し、既存の
`Plots` / Plotly 表示基盤で確認できる最小到達点を対象にする。

## Completion Status

milestone 33 / Issue #7360 の README visualization MVP は完了済み。対応済み surface:
`using OrdinaryDiffEq` / `using SciMLBase`、`ODEProblem`、`Tsit5()`、adaptive
`solve(prob, Tsit5(); dt, saveat, reltol, abstol)`、`ODESolution` fields、`plot(sol)`、
`plot(sol, idxs=(1,2,3))`、`plot!(sol.t, f)`、Plotly artifact routing、
iOS/Web/Flutter README samples。MVP 外の broader SciML / OrdinaryDiffEq / Plots
parity は follow-up #7865 に集約し、各ギャップを個別の実装 Issue
(#7981–#7987) へ昇格させた。下の "Promoted Follow-up Issues" を参照。

参照ソース:

- `extern/OrdinaryDiffEq.jl/README.md:34-47`: 線形 ODE の README サンプル。
- `extern/OrdinaryDiffEq.jl/README.md:49-64`: Lorenz in-place ODE の README サンプル。
- `extern/OrdinaryDiffEq.jl/README.md:66-77`: StaticArrays 版。MVP 外。
- `extern/OrdinaryDiffEq.jl/README.md:79-123`: refined ODE / second-order / symplectic
  examples。MVP 外。
- `extern/SciMLBase.jl/src/problems/ode_problems.jl:57-63`: upstream `ODEProblem`
  field surface (`f`, `u0`, `tspan`, `p`, `kwargs`)。
- `extern/SciMLBase.jl/src/solutions/ode_solutions.jl:92-107`: upstream `ODESolution`
  field surface (`u`, `t`, `prob`, `alg`, `stats`, `retcode`)。
- `extern/OrdinaryDiffEq.jl/src/OrdinaryDiffEq.jl:43-69`: upstream exports for
  solve/init/step, problem types, callbacks, utilities, and algorithms.
- `extern/SciMLBase.jl/src/SciMLBase.jl:998-1013`: broader SciMLBase problem /
  solution exports that remain outside this MVP.

## MVP Scope

MVP の受け入れ対象は README の次の 2 系統に固定する。

### 線形 ODE

README の形を保つ対象:

```julia
using OrdinaryDiffEq
f(u, p, t) = 1.01 * u
u0 = 1 / 2
tspan = (0.0, 1.0)
prob = ODEProblem(f, u0, tspan)
sol = solve(prob, Tsit5(), reltol = 1e-8, abstol = 1e-8)
using Plots
plot(sol, linewidth = 5, title = "Solution to the linear ODE with a thick line",
    xaxis = "Time (t)", yaxis = "u(t)", label = "My Thick Line!")
plot!(sol.t, t -> 0.5 * exp(1.01 * t), lw = 3, ls = :dash, label = "True Solution!")
```

sjulia MVP では Unicode 表示単位など、描画結果に不要な文字列差分は fixture で簡略化
してよい。実行形としては `using OrdinaryDiffEq`、`ODEProblem`、`solve(prob,
Tsit5(); reltol, abstol)`、`sol.t`、`plot(sol, ...)`、`plot!(sol.t, t -> ...)` が通る
ことを到達条件にする。

### Lorenz In-Place ODE

README の形を保つ対象:

```julia
using OrdinaryDiffEq
function lorenz!(du, u, p, t)
    du[1] = 10.0 * (u[2] - u[1])
    du[2] = u[1] * (28.0 - u[3]) - u[2]
    du[3] = u[1] * u[2] - (8 / 3) * u[3]
end
u0 = [1.0; 0.0; 0.0]
tspan = (0.0, 100.0)
prob = ODEProblem(lorenz!, u0, tspan)
sol = solve(prob, Tsit5())
using Plots
plot(sol, idxs = (1, 2, 3))
```

到達条件は vector `u0`、in-place RHS `f(du, u, p, t)`、`sol.t` / `sol.u` の構築、
および `plot(sol, idxs=(1,2,3))` による 3D 系列表示である。

## Required API Surface

後続 Phase は次の API を実装対象にする。

| API | MVP 要件 |
|---|---|
| `using OrdinaryDiffEq` | bundled package loader で解決できる。 |
| `using SciMLBase` | `ODEProblem` / `ODESolution` の所有元として解決できる。 |
| `ODEProblem(f, u0, tspan)` | out-of-place scalar RHS と in-place vector RHS を受け付ける。 |
| `ODEProblem(f, u0, tspan, p)` | `p` を保持する。README MVP では省略時 `nothing` 相当でよい。 |
| `Tsit5()` | README 互換の algorithm object。Phase #7367 以降は Tsit5 tableau backend で実行する。 |
| `solve(prob, Tsit5(); reltol, abstol)` | adaptive step controller の tolerance として使う。 |
| `solve(prob, Tsit5(); dt, saveat)` | `dt` は初期 step、`saveat` は保存 grid を制御する。 |
| `ODESolution` | 少なくとも `u`, `t`, `prob`, `alg`, `retcode`, `stats` を持ち、field access できる。 |
| `plot(sol; kwargs...)` | `sol.t` と `sol.u` から Plotly series を生成する。 |
| `plot(sol, idxs=(...); kwargs...)` | Lorenz の `(1,2,3)` 3D 軌道を生成する。 |
| `plot!(sol.t, f; kwargs...)` | 既存 Plots.jl の function overlay 経路で解析解を重ねられる。 |

## Supported API Matrix

Phase 5 (#7366) 時点の public surface は、README 2 サンプルを通す最小 subset に
固定する。

| Surface | Supported | Notes |
|---|---:|---|
| `using OrdinaryDiffEq` / `using SciMLBase` | yes | bundled package loader で解決する。 |
| `ODEProblem(f, u0, tspan)` | yes | scalar out-of-place と vector in-place RHS を対象にする。 |
| `ODEProblem(f, u0, tspan, p)` | yes | `p` を保持し、RHS 呼び出しへ渡す。 |
| `ODEProblem` fields | partial | `f`, `u0`, `tspan`, `p`, `kwargs`, `isinplace` を公開する。 |
| `Tsit5()` | yes | Tsitouras 5/4 tableau backend。stage/step limiter fields は保持するが未使用。 |
| `solve(prob, Tsit5(); dt, saveat, reltol, abstol)` | partial | `dt` は initial internal step、`saveat` は output grid、`reltol` / `abstol` は adaptive error control に使う。 |
| `ODESolution` fields | partial | `u`, `t`, `prob`, `alg`, `stats`, `retcode` を公開する。`retcode` は実 `ReturnCode` 値（`ReturnCode.Success` 等, #7981）。連続補間は callable `sol(t)`(線形補間, #7982)で提供。 |
| `ReturnCode` / `successful_retcode` | yes | #7981。`ReturnCode.Success`/`Terminated`/`Failure`/… を struct-namespace で提供（module ではなく struct: alias 経由の member access が sjulia で効くため）。`sol.retcode === ReturnCode.Success`、`successful_retcode(sol)`/`(rc)` が動作。旧 `:Success` symbol も `successful_retcode` で parity 維持。 |
| `plot(sol)` | yes | scalar time series または vector component time series。 |
| `plot(sol, idxs=(1,2))` / `(1,2,3)` | yes | 2D phase path / Plotly 3D path。 |
| `plot!(sol.t, f)` | yes | README linear ODE の解析解 overlay 用。 |
| `init`, `step!`, `solve!`, `reinit!`, `remake`, `successful_retcode` | yes | integrator interface subset #7981。`step!(integ)` は次の出力点まで進め、`step!(integ, dt, stop_at_tdt)` は `dt` だけ進める。`solve!(init(prob, alg; ...))` は `solve(...)` を再現。`solve`/`init` は `tstops=[...]` を受け、要求時刻に step を着地させる（MVP では tstops を保存グリッドへマージ）。`ReturnCode` 値・`successful_retcode` も実装済み（上行）。 |
| callbacks / events (`ContinuousCallback`, `DiscreteCallback`, `CallbackSet`) | partial | #7983。`solve(prob, alg; callback=...)` の fixed-step RK4 経路で event 検出（bisection root-find）。`VectorContinuousCallback` / adaptive 経路 / `save_positions` は残。 |
| dense output / `sol(t)` interpolation beyond saved grid | partial | callable `ODESolution` #7982。`sol(t)` / `sol(t; idxs=...)` / `sol(ts)` を**線形補間**で提供（保存点間）。Tsit5 4 次 dense interpolant は #7982 Phase B 残。 |
| `SecondOrderODEProblem`, symplectic / refined ODE examples | partial | #7985。`SecondOrderODEProblem(f, du0, u0, tspan)` + `VelocityVerlet()` symplectic 積分（保存状態は `[du...; u...]`）。`ArrayPartition` / 高次 symplectic / refined examples は残。 |
| StaticArrays variant / static-state specialization | yes | #7984。out-of-place `@SVector` RHS + `@SVector` 初期状態を `solve(prob, Tsit5(); ...)` で解け、static 要素型は end-to-end で保持される（`Vector` へ silent widening しない）。性能注記: この VM では out-of-place の `SVector` 確保が in-place buffered `Vector` 経路より**遅い**ため、Lorenz サンプルは in-place のまま（#8094）。mixed `SVector .- Vector` broadcast は #8161。 |
| views (`SubArray`) / broader SciML array surfaces | partial | #7986。`view`/`SubArray` 状態は solve 開始時に dense `Vector` へ densify され、in-place / out-of-place / integrator interface で plain `Vector` と同一 trajectory を返す（backing buffer は非破壊。upstream も u0 を内部 dense コピーする）。sparse は同じ規則で densify されるが、bundled `SparseArrays` subset が `sparse`/`sparsevec` 未実装のため sparse 状態は solver に到達できない（densify 方針を文書化）。out-of-place vector RHS の buffered-path 回帰は #8163 で修正。 |
| Plots recipe pipeline instead of direct `plot(sol)` conversion | yes | #7987。`plot(sol)`/`plot!(sol)` は `apply_recipe`(AbstractODESolution に登録された recipe)を経由して `Series` を生成・assemble する（hard-coded special case を廃止、artifact 形状は無回帰）。recipe 属性 `idxs`(成分/phase) / `vars`(idxs の上流別名) / `denseplot`+`plotdensity`(callable `sol(t)` #7982 をfine gridでサンプル)が pipeline を流れる。完全な `RecipesBase.@recipe` マクロは未実装（sjulia のクロスモジュール抽象 dispatch 制約のため、recipe 登録は concrete `plot` entry + 抽象 `apply_recipe` の薄い indirection で代替）。 |

## Solver Decision

Phase #7367 以降の solver backend は Tsitouras 5/4 tableau の adaptive stepper とする。
public API は README と同じ `solve(prob, Tsit5(); ...)` を維持する。

- Phase #7363 の fixed-step RK4 compatibility backend は Phase #7367 で置き換えた。
- `dt` は最初の internal step size として扱い、accepted/rejected step ごとに controller が
  次の internal step を調整する。
- `saveat` は `sol.t` / `sol.u` の保存 grid を決める。内部 step は必要に応じて
  `saveat` 区間内で分割される。保存点間の連続出力は callable `sol(t)` の**線形補間**
  で提供する（#7982）。Tsit5 の 4 次 dense interpolant は #7982 Phase B 残スコープ。
- `reltol` / `abstol` は embedded error estimate の scale として使い、tight tolerance
  ほど accepted internal steps / RHS evaluations が増える。
- `stats` は `:algorithm => :Tsit5`、`:steps`、`:attempts`、`:rejected_steps`、
  `:rhs_evals` を持つ。

### 性能 / Benchmark (#8094)

- vector in-place RHS では Tsit5 stepper は **reusable stage buffers**
  (`k2…k7`, `tmp`, `unew`, `err`) を 1 回確保して step ごとに再利用する
  (`_tsit5_solve_interval_buffered`)。step あたりの `copy(u)` を排し、Lorenz
  iOS サンプルの solve を高速化した（#8094）。
- 回帰検出用に `cargo bench -p subset_julia_vm --bench vm_ode_tsit5_lorenz_benchmark`
  を追加。`benchmarks/vm_ode_tsit5_lorenz.jl`(Lorenz, `dt=0.02`, `saveat=0.02`,
  `tspan=(0,20)`) の `solve` を `Vm::run()` だけで測る（parse/lower/compile と
  Plots artifact を除く）。
- **静的状態の性能注記**: `SVector` 状態（out-of-place）は正しく解け型も保持するが、
  毎 stage で新しい `SVector` を確保するため、この VM では in-place buffered
  `Vector` 経路より**遅い**。よって README Lorenz サンプルは in-place の
  `Vector` RHS を維持する（#8094 提案 #1 の「StaticArrays 高速化」はこの VM では
  逆効果）。

## Parity And Regression Policy

README MVP の regression は、upstream OrdinaryDiffEq の full display/value parity ではなく、
sjulia が実装した supported surface の構造を固定する。

- upstream Julia/OrdinaryDiffEq は README source shape と expected user workflow の基準にする。
- sjulia fixtures は `ODEProblem` / `ODESolution` fields、representative scalar/vector
  trajectory values、`Plot` / `Series` shape、Plotly MIME routing を固定する。
- upstream Tsit5 と sjulia Tsit5 の exact internal step sequence parity は要求しない。
  README-level linear/Lorenz values、tolerance-sensitive step count、and saved grid shape を
  conservative tolerance で固定する。
- upstream `ODESolution` text display は `retcode`、interpolation summary、`t`、`u` を
  表示するが、sjulia MVP は field access と plot artifact を regression 対象にし、
  interpolation summary は持たない。
- README sample の app-surface regression は iOS/Web/Flutter sample catalog と
  `application/vnd.plotly+json` artifact routing を確認する。

## Non-Goals

次は milestone 33 の MVP 外であり、後続 Issue で明示的に昇格されるまで実装条件にしない。

- full SciMLBase / OrdinaryDiffEq dependency tree の移植。
- callbacks / events、dense output interpolation、StaticArrays 版 README サンプル、
  sparse arrays / views、`SecondOrderODEProblem`、symplectic integrators、refined ODE
  examples、full Plots recipe pipeline、sensitivity analysis、GPU、stiff solvers、
  DiffEq ecosystem 全体の API parity は follow-up Issue #7865 で追跡し、下表の
  個別 Issue (#7981–#7987) へ昇格済み。sensitivity analysis / GPU / stiff solvers /
  DiffEq ecosystem 全体は引き続き未昇格の long-tail。

## Promoted Follow-up Issues

#7865 (tracking) を、各ギャップごとの実装 Issue に分解した。各 Issue は upstream 参照・
fixture 計画・app-surface 影響・受け入れ条件を持つ。これらが本 README MVP の
non-goal を構成する。

| Gap | Issue | 主な surface |
|---|---|---|
| SciML integrator interface | #7981 | `init` / `step!` / `solve!` / `remake` / `reinit!` / `tstops` / retcode helpers |
| Dense output / interpolation | #7982 | callable `ODESolution` (`sol(t)`, `sol(t; idxs=...)`) |
| Callbacks & events | #7983 | `CallbackSet` / `ContinuousCallback` / `DiscreteCallback` / `VectorContinuousCallback` |
| StaticArrays variant | #7984 | `@SVector` 状態と static-state specialization (milestone #36 / #7433) |
| Second-order / symplectic | #7985 | `SecondOrderODEProblem` と symplectic solver subset |
| Broader array surfaces | #7986 | views / sparse states を stepper helper に通す |
| Plots recipe pipeline | #7987 | `RecipesBase.@recipe` 経由の `plot(sol)` |

`#7367` は引き続き adaptive Tsit5 backend 専用 Issue として残し、上記には統合しない。

## Plot Support

Phase 3 (#7364) では bundled `Plots` が `SciMLBase.ODESolution` を直接 `Plot` /
`Series` 値へ変換していた。#7987 でこの hard-coded special case を **recipe
pipeline** へ置き換えた: `apply_recipe(sol; idxs, vars, denseplot, plotdensity,
label)`(AbstractODESolution に登録された recipe)が `Series` のリストと 3D ヒント
を返し、`plot`/`plot!` がそれを apply して `Plot` を assemble する。artifact 形状は
従来の直接変換と同一(無回帰)。

- `plot(sol)` は scalar solution では `sol.t` vs `sol.u`、vector solution では各
  component の time series を 1 series ずつ生成する。
- `plot(sol, idxs=(1,2))` は 2D phase path、`plot(sol, idxs=(1,2,3))` は `:path3d`
  series(Plotly 3D path)。`vars=` は `idxs` の上流別名。
- `plot(sol, denseplot=true, plotdensity=N)` は callable `sol(t)`(#7982)を `N` 点の
  fine grid でサンプルし、滑らかな曲線を生成する。
- `plot!(sol)` は recipe series を current plot へ overlay する。
- `plot!(sol.t, t -> ...)` は既存 function overlay 経路を使う。
- 完全な `RecipesBase.@recipe` マクロは未実装: sjulia のクロスモジュール抽象
  dispatch 制約のため、generic な `plot(x)`→`apply_recipe` 経路は使わず、concrete
  `plot(::ODESolution)` entry が 抽象 `apply_recipe(::AbstractODESolution)` へ委譲する
  薄い indirection で recipe 機構を実現している。
- `linewidth` / `lw` / `ls` / `xaxis` / `yaxis` / `label` などの README 表示
  keyword は受け付けるが、現 Plots subset では style metadata として保持しない。

## Sample Surfaces

Phase 4 (#7365) では README MVP sample を iOS/Web/Flutter に登録する。sample code は
README に近い形を保ちつつ、MVP solver の安定性のため `dt` / `saveat` を明示する。

- `ordinarydiffeq_linear_ode`: linear ODE を `solve(prob, Tsit5(), dt=0.1,
  reltol=1e-8, abstol=1e-8)` で解き、`plot(sol, ...)` に解析解
  `plot!(sol.t, t -> ...)` を重ねる。
- `ordinarydiffeq_lorenz_attractor`: Lorenz in-place RHS を `dt=0.02,
  saveat=0.02` で解き、`plot(sol, idxs=(1,2,3))` を final expression とする。
- どちらも final expression は通常の `Plots.Plot` なので、既存の
  `application/vnd.plotly+json` MIME routing を使う。Swift fallback sample は不要。

## Fixture And Sample Targets

後続 Phase は次の名前と配置を使う。

| 用途 | 配置 | 名前 |
|---|---|---|
| package skeleton fixture | `subset_julia_vm/tests/fixtures/packages/` | `ordinarydiffeq_skeleton_7362.jl` |
| scalar solve fixture | `subset_julia_vm/tests/fixtures/packages/` | `ordinarydiffeq_linear_solve_7363.jl` |
| Lorenz solve fixture | `subset_julia_vm/tests/fixtures/packages/` | `ordinarydiffeq_lorenz_solve_7363.jl` |
| plot fixture | `subset_julia_vm/tests/fixtures/packages/` | `ordinarydiffeq_plot_solution_7364.jl` |
| README MVP completion fixture | `subset_julia_vm/tests/fixtures/packages/` | `ordinarydiffeq_readme_mvp_7366.jl` |
| Tsit5 adaptive fixture | `subset_julia_vm/tests/fixtures/packages/` | `ordinarydiffeq_tsit5_adaptive_7367.jl` |
| iOS linear sample | `SubsetJuliaVMApp/SubsetJuliaVMApp/Resources/Samples/intermediate/` | `ordinarydiffeq_linear_ode.jl` |
| iOS Lorenz sample | `SubsetJuliaVMApp/SubsetJuliaVMApp/Resources/Samples/intermediate/` | `ordinarydiffeq_lorenz_attractor.jl` |
| Flutter linear sample | `mobile/assets/samples/intermediate/` | `ordinarydiffeq_linear_ode.jl` |
| Flutter Lorenz sample | `mobile/assets/samples/intermediate/` | `ordinarydiffeq_lorenz_attractor.jl` |
| web samples | `web/samples_ir.js` | `ordinarydiffeq_linear_ode`, `ordinarydiffeq_lorenz_attractor` |

Fixture manifest names must keep the category prefix, for example
`packages_ordinarydiffeq_skeleton_7362`.

## Phase Acceptance Gates

- Phase 1 (#7362): `using OrdinaryDiffEq`, scalar/vector `ODEProblem`, and `Tsit5()`
  construction pass under the packages fixture category.
- Phase 2 (#7363): linear ODE and Lorenz fixtures produce stable `ODESolution`
  fields with expected grid lengths and representative values.
- Phase 3 (#7364): `plot(sol)`, `plot(sol, idxs=(1,2,3))`, and
  `plot!(sol.t, t -> ...)` produce Plotly artifacts through the existing Plots.jl path.
- Phase 4 (#7365): iOS/Web/Flutter expose the two README MVP samples and their
  fixture-backed code paths.
- Phase 5 (#7366): docs, known parity gaps, and regression coverage are updated;
  milestone non-goals remain explicit.
- Follow-up (#7367): `Tsit5()` uses the Tsitouras 5/4 tableau backend, and
  `reltol` / `abstol` affect adaptive internal steps while `saveat` remains stable.
