module OrdinaryDiffEq

# README-facing facade for the OrdinaryDiffEq MVP. Upstream OrdinaryDiffEq
# imports the problem/solution surface from SciMLBase and exports widely-used
# algorithms such as Tsit5; the MVP keeps that shape but defers solving to
# Issue #7363.

import SciMLBase
# Workaround: re-export the `Tsit5` algorithm type from SciMLBase rather than
# defining it here, and register its `solve` dispatch on `SciMLBase.solve`
# (in SciMLBase.jl). sjulia cannot extend another module's function from this
# module — neither `function SciMLBase.solve(...)` (lowering "missing function
# name") nor `import SciMLBase: solve; function solve(...)` (creates a separate
# `OrdinaryDiffEq.solve` instead of extending) works — so the `Tsit5` method
# must live with `solve` in SciMLBase, and `Tsit5` must therefore live there too
# (Issue #8052). Importing the type (not `const Tsit5 = SciMLBase.Tsit5`, which
# fails as a constructor — Issue #8049) keeps `Tsit5()` / `x isa Tsit5` working;
# the only casualty is qualified `OrdinaryDiffEq.Tsit5` access (Issue #8053).
import SciMLBase: Tsit5
# `ReturnCode` is a submodule of SciMLBase (Issue #7981); alias it so
# `ReturnCode.Success` etc. resolve under `using OrdinaryDiffEq`. `import
# SciMLBase: ReturnCode` does not bind a submodule in sjulia; a `const` alias to
# the module value does (it is a plain binding, not a type constructor — the
# #8049 caveat only bites type aliases).
const ReturnCode = SciMLBase.ReturnCode

export SciMLBase, solve, ODEProblem, ODESolution, Tsit5, ReturnCode,
       SecondOrderODEProblem, VelocityVerlet,
       init, step!, solve!, reinit!, remake, successful_retcode,
       DiscreteCallback, ContinuousCallback, CallbackSet

ODEProblem(args...; kwargs...) = SciMLBase.ODEProblem(args...; kwargs...)
ODESolution(args...; kwargs...) = SciMLBase.ODESolution(args...; kwargs...)
SecondOrderODEProblem(args...; kwargs...) = SciMLBase.SecondOrderODEProblem(args...; kwargs...)

# Symplectic velocity-Verlet integrator for SecondOrderODEProblem (Issue #7985).
# Stays local: SciMLBase.solve(::SecondOrderODEProblem, alg) accepts any alg and
# does not reject unknown algorithms, so it needs no alg-type dispatch here.
struct VelocityVerlet end

# `solve` is a thin forwarder onto `SciMLBase.solve`, where all alg dispatch
# lives: the `Tsit5` method (Issue #7996), the generic unsupported-alg error,
# the SecondOrderODEProblem path, and the `args...` catch-all. Forwarding (not a
# separate dispatching `OrdinaryDiffEq.solve`) means `solve(prob, Tsit5())` and
# the qualified `SciMLBase.solve(prob, Tsit5())` reach one method table, so the
# qualified entry point no longer regresses to the error fallback (Issue #8050).
solve(args...; kwargs...) = SciMLBase.solve(args...; kwargs...)

# Integrator interface subset (Issue #7981), forwarded from SciMLBase.
init(args...; kwargs...) = SciMLBase.init(args...; kwargs...)
step!(args...; kwargs...) = SciMLBase.step!(args...; kwargs...)
solve!(args...; kwargs...) = SciMLBase.solve!(args...; kwargs...)
reinit!(args...; kwargs...) = SciMLBase.reinit!(args...; kwargs...)
remake(args...; kwargs...) = SciMLBase.remake(args...; kwargs...)
successful_retcode(args...; kwargs...) = SciMLBase.successful_retcode(args...; kwargs...)

# Callbacks & events (Issue #7983), forwarded from SciMLBase.
DiscreteCallback(args...; kwargs...) = SciMLBase.DiscreteCallback(args...; kwargs...)
ContinuousCallback(args...; kwargs...) = SciMLBase.ContinuousCallback(args...; kwargs...)
CallbackSet(args...; kwargs...) = SciMLBase.CallbackSet(args...; kwargs...)

# SVector Tsit5 solve (Issue #7984): override _tsit5_solve with a version that
# compiles in the OrdinaryDiffEq module (where StaticArrays is available),
# avoiding pre-compilation artefacts in SciMLBase.
function SciMLBase._tsit5_solve(prob::SciMLBase.ODEProblem, alg::SciMLBase.Tsit5; dt=nothing, saveat=nothing, reltol=nothing, abstol=nothing, callback=nothing, tstops=nothing)
    t0 = prob.tspan[1]
    t1 = prob.tspan[2]
    if callback !== nothing
        h = dt === nothing ? (t1 - t0) / 1000 : dt
        return SciMLBase._solve_with_callbacks(prob, alg, SciMLBase._callbacks(callback), h, t0, t1)
    end
    # Merge user `tstops` into the step/output grid so a step lands on each (#7981).
    ts = SciMLBase._merge_tstops(SciMLBase._solve_grid(t0, t1, dt, saveat), tstops, t0, t1)
    reltol = reltol === nothing ? 1e-3 : reltol
    abstol = abstol === nothing ? 1e-6 : abstol
    # Broader SciML array surfaces (Issue #7986): densify a `view`/`SubArray`
    # initial state to a dense `Vector` before integration. In sjulia a `SubArray`
    # reports `ismutable == false`, so without this it would skip the in-place fast
    # path and hit `SubArray + Vector` operator gaps on the generic stepper path.
    # `collect` yields a fresh dense copy (the user's backing buffer is never
    # mutated, matching upstream). Static states (`SVector`) and scalars are left
    # untouched so the static-state path (#7984) keeps its element type. This is
    # the LIVE `_tsit5_solve` (it overrides `SciMLBase._tsit5_solve` so the
    # `SVector` path compiles in this module; #8104), so the densify must live here.
    u0src = prob.u0 isa SubArray ? collect(prob.u0) : prob.u0
    # Workaround: SciMLBase._copy_state qualified call does not dispatch to the
    # AbstractVector method from OrdinaryDiffEq context (Issue #8104). Use copy(u)
    # directly for mutable arrays; immutable states (SVector, scalars) are safe to alias.
    u = ismutable(u0src) ? copy(u0src) : u0src
    t = t0
    h = dt === nothing ? (length(ts) > 1 ? ts[2] - ts[1] : t1 - t0) : dt
    k1 = SciMLBase._rhs(prob, u, t)
    us = Any[]
    push!(us, ismutable(u) ? copy(u) : u)
    stats = Dict(:algorithm => :Tsit5, :steps => 0, :attempts => 0, :rejected_steps => 0, :rhs_evals => 1)
    # Buffered fast path only for IN-PLACE RHS: the reusable `k2…k7` buffers are
    # filled by `SciMLBase._rhs!`, which no-ops on the buffer for an out-of-place
    # RHS (it returns a fresh array the buffered trial ignores), silently using
    # stale stages. Out-of-place states take the generic operator path (Issue #8163).
    if prob.isinplace && u isa AbstractVector && ismutable(u)
        k2 = copy(u); k3 = copy(u); k4 = copy(u); k5 = copy(u)
        k6 = copy(u); k7 = copy(u); tmp = copy(u)
        unew_buf = copy(u); err_buf = copy(u)
        for i in 2:length(ts)
            u, t, h, k1 = SciMLBase._tsit5_solve_interval_buffered(prob, u, t, ts[i], h, k1, reltol, abstol, stats, k2, k3, k4, k5, k6, k7, tmp, unew_buf, err_buf)
            push!(us, copy(u))
        end
    else
        for i in 2:length(ts)
            u, t, h, k1 = SciMLBase._tsit5_solve_interval(prob, u, t, ts[i], h, k1, reltol, abstol, stats)
            push!(us, u)
        end
    end
    return SciMLBase.ODESolution(us, ts, prob, alg; stats=stats, retcode=SciMLBase.ReturnCode.Success)
end

end # module OrdinaryDiffEq
