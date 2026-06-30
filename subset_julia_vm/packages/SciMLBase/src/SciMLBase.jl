module SciMLBase

# Minimal SciMLBase subset for the OrdinaryDiffEq README visualization MVP
# (Issue #7360). This keeps the upstream public names and field surface needed by
# the README samples, without pulling in the full SciML dependency graph.

export solve, ODEProblem, ODESolution, SecondOrderODEProblem,
       AbstractDEProblem, AbstractODEProblem,
       AbstractDESolution, AbstractODESolution,
       NullParameters, Tsit5, ReturnCode

abstract type AbstractDEProblem end
abstract type AbstractODEProblem <: AbstractDEProblem end

abstract type AbstractDESolution end
abstract type AbstractODESolution <: AbstractDESolution end

struct NullParameters end

# ReturnCode enum subset (Issue #7981). Upstream SciMLBase exposes `ReturnCode`
# as an EnumX enum whose values (`ReturnCode.Success`, `ReturnCode.Failure`, …)
# are the real `sol.retcode`. Mirror that `ReturnCode.<Name>` surface so
# `sol.retcode` is a real return-code value (not the MVP `:Success` symbol) and
# `sol.retcode === ReturnCode.Success` works.
#
# `ReturnCode` is a STRUCT-instance namespace, not a `module`: sjulia does not
# resolve module member access (`ReturnCode.Success`) through a re-exported/`const`
# alias of a module — `OrdinaryDiffEq` aliases this into its own scope, and struct
# FIELD access works through such an alias where module member access does not.
# Each value is a `ReturnCodeValue` carrying its `name`; the singletons live in the
# one `const ReturnCode` instance, so `ReturnCode.Success === ReturnCode.Success`.
struct ReturnCodeValue
    name::Symbol
end

struct ReturnCodeNamespace
    Default::ReturnCodeValue           # solver constructed, not yet run
    Success::ReturnCodeValue           # reached the end of tspan
    Terminated::ReturnCodeValue        # stopped early by a callback, still OK
    MaxIters::ReturnCodeValue
    DtLessThanMin::ReturnCodeValue
    Unstable::ReturnCodeValue
    Failure::ReturnCodeValue
end

const ReturnCode = ReturnCodeNamespace(
    ReturnCodeValue(:Default), ReturnCodeValue(:Success), ReturnCodeValue(:Terminated),
    ReturnCodeValue(:MaxIters), ReturnCodeValue(:DtLessThanMin),
    ReturnCodeValue(:Unstable), ReturnCodeValue(:Failure),
)

struct ODEProblem <: AbstractODEProblem
    f
    u0
    tspan
    p
    kwargs
    isinplace
end

function _ode_isinplace(f, u0, tspan, p)
    t0 = tspan[1]
    return hasmethod(f, Tuple{typeof(u0), typeof(u0), typeof(p), typeof(t0)})
end

function ODEProblem(f, u0, tspan, p=NullParameters(); kwargs...)
    return ODEProblem(f, u0, tspan, p, kwargs, _ode_isinplace(f, u0, tspan, p))
end

struct ODESolution <: AbstractODESolution
    u
    t
    prob
    alg
    stats
    retcode
end

function ODESolution(u, t, prob, alg; stats=nothing, retcode=ReturnCode.Default)
    return ODESolution(u, t, prob, alg, stats, retcode)
end

_copy_state(u::AbstractVector) = copy(u)
_copy_state(u) = u

# Broader SciML array surfaces (Issue #7986): normalize a non-dense `AbstractVector`
# initial state to a plain dense `Vector` before integration. A `view`/`SubArray`
# state otherwise (a) reports `ismutable == false` in sjulia so it skips the
# in-place-buffered fast path, and (b) hits `SubArray + Vector` operator gaps on the
# generic path. Densifying matches upstream, which copies `u0` into dense internal
# storage for the integration (the user's backing buffer is never mutated). Static
# arrays (`SVector`) and scalars are left untouched so the static-state path (#7984)
# keeps its element type. Sparse vectors would be densified by the same rule; the
# bundled `SparseArrays` subset does not yet implement `sparse`/`sparsevec`
# constructors, so a sparse state cannot currently reach the solver (documented
# densify decision per #7986). A single runtime `isa` branch is used rather than a
# `_densify_state(::SubArray)` method to avoid the specialization-dependent
# mis-dispatch seen in #8158.
function _densify_state(u0)
    if u0 isa SubArray
        return collect(u0)
    end
    return u0
end

function _zero_like(u::AbstractVector)
    out = copy(u)
    for i in 1:length(out)
        out[i] = zero(out[i])
    end
    return out
end

_zero_like(u) = zero(u)

function _state_add_scaled(u::AbstractVector, a, k::AbstractVector)
    out = copy(u)
    for i in 1:length(out)
        out[i] = u[i] + a * k[i]
    end
    return out
end

_state_add_scaled(u, a, k) = u + a * k

function _state_add_scaled!(out, u::AbstractVector, a, k::AbstractVector)
    for i in 1:length(out)
        out[i] = u[i] + a * k[i]
    end
    return out
end

function _rhs(prob::ODEProblem, u, t)
    if prob.isinplace
        du = _zero_like(u)
        prob.f(du, u, prob.p, t)
        return du
    end
    return prob.f(u, prob.p, t)
end

function _rhs!(du, prob::ODEProblem, u, t)
    if prob.isinplace
        prob.f(du, u, prob.p, t)
        return du
    end
    return prob.f(u, prob.p, t)
end

function _state_add2(u::AbstractVector, dt, a1, k1, a2, k2)
    out = copy(u)
    for i in 1:length(out)
        out[i] = u[i] + dt * (a1 * k1[i] + a2 * k2[i])
    end
    return out
end

_state_add2(u, dt, a1, k1, a2, k2) = u + dt * (a1 * k1 + a2 * k2)

function _state_add2!(out, u::AbstractVector, dt, a1, k1, a2, k2)
    for i in 1:length(out)
        out[i] = u[i] + dt * (a1 * k1[i] + a2 * k2[i])
    end
    return out
end

function _state_add3(u::AbstractVector, dt, a1, k1, a2, k2, a3, k3)
    out = copy(u)
    for i in 1:length(out)
        out[i] = u[i] + dt * (a1 * k1[i] + a2 * k2[i] + a3 * k3[i])
    end
    return out
end

_state_add3(u, dt, a1, k1, a2, k2, a3, k3) = u + dt * (a1 * k1 + a2 * k2 + a3 * k3)

function _state_add3!(out, u::AbstractVector, dt, a1, k1, a2, k2, a3, k3)
    for i in 1:length(out)
        out[i] = u[i] + dt * (a1 * k1[i] + a2 * k2[i] + a3 * k3[i])
    end
    return out
end

function _state_add4(u::AbstractVector, dt, a1, k1, a2, k2, a3, k3, a4, k4)
    out = copy(u)
    for i in 1:length(out)
        out[i] = u[i] + dt * (a1 * k1[i] + a2 * k2[i] + a3 * k3[i] + a4 * k4[i])
    end
    return out
end

_state_add4(u, dt, a1, k1, a2, k2, a3, k3, a4, k4) =
    u + dt * (a1 * k1 + a2 * k2 + a3 * k3 + a4 * k4)

function _state_add4!(out, u::AbstractVector, dt, a1, k1, a2, k2, a3, k3, a4, k4)
    for i in 1:length(out)
        out[i] = u[i] + dt * (a1 * k1[i] + a2 * k2[i] + a3 * k3[i] + a4 * k4[i])
    end
    return out
end

function _state_add5(u::AbstractVector, dt, a1, k1, a2, k2, a3, k3, a4, k4, a5, k5)
    out = copy(u)
    for i in 1:length(out)
        out[i] = u[i] + dt * (a1 * k1[i] + a2 * k2[i] + a3 * k3[i] + a4 * k4[i] + a5 * k5[i])
    end
    return out
end

_state_add5(u, dt, a1, k1, a2, k2, a3, k3, a4, k4, a5, k5) =
    u + dt * (a1 * k1 + a2 * k2 + a3 * k3 + a4 * k4 + a5 * k5)

function _state_add5!(out, u::AbstractVector, dt, a1, k1, a2, k2, a3, k3, a4, k4, a5, k5)
    for i in 1:length(out)
        out[i] = u[i] + dt * (a1 * k1[i] + a2 * k2[i] + a3 * k3[i] + a4 * k4[i] + a5 * k5[i])
    end
    return out
end

function _state_add6(u::AbstractVector, dt, a1, k1, a2, k2, a3, k3, a4, k4, a5, k5, a6, k6)
    out = copy(u)
    for i in 1:length(out)
        out[i] = u[i] + dt * (a1 * k1[i] + a2 * k2[i] + a3 * k3[i] + a4 * k4[i] + a5 * k5[i] + a6 * k6[i])
    end
    return out
end

_state_add6(u, dt, a1, k1, a2, k2, a3, k3, a4, k4, a5, k5, a6, k6) =
    u + dt * (a1 * k1 + a2 * k2 + a3 * k3 + a4 * k4 + a5 * k5 + a6 * k6)

function _state_add6!(out, u::AbstractVector, dt, a1, k1, a2, k2, a3, k3, a4, k4, a5, k5, a6, k6)
    for i in 1:length(out)
        out[i] = u[i] + dt * (a1 * k1[i] + a2 * k2[i] + a3 * k3[i] + a4 * k4[i] + a5 * k5[i] + a6 * k6[i])
    end
    return out
end

function _state_combo7(dt, b1, k1, b2, k2, b3, k3, b4, k4, b5, k5, b6, k6, b7, k7)
    return dt * (b1 * k1 + b2 * k2 + b3 * k3 + b4 * k4 + b5 * k5 + b6 * k6 + b7 * k7)
end

function _state_combo7(dt, b1, k1::AbstractVector, b2, k2, b3, k3, b4, k4, b5, k5, b6, k6, b7, k7)
    out = copy(k1)
    for i in 1:length(out)
        out[i] = dt * (b1 * k1[i] + b2 * k2[i] + b3 * k3[i] + b4 * k4[i] + b5 * k5[i] + b6 * k6[i] + b7 * k7[i])
    end
    return out
end

function _state_combo7!(out, dt, b1, k1::AbstractVector, b2, k2, b3, k3, b4, k4, b5, k5, b6, k6, b7, k7)
    for i in 1:length(out)
        out[i] = dt * (b1 * k1[i] + b2 * k2[i] + b3 * k3[i] + b4 * k4[i] + b5 * k5[i] + b6 * k6[i] + b7 * k7[i])
    end
    return out
end

function _tsit5_trial(prob::ODEProblem, u, t, dt, k1)
    # Inline arithmetic to avoid CallTypedDispatch resolving to AbstractVector
    # methods for immutable types like SVector (Issue #7984). The + and * operators
    # dispatch correctly for both scalars and static arrays.
    k2 = _rhs(prob, u + (dt * 0.161) * k1, t + 0.161 * dt)
    k3 = _rhs(prob, u + dt * (-0.008480655492356989 * k1 + 0.335480655492357 * k2), t + 0.327 * dt)
    k4 = _rhs(prob, u + dt * (2.8971530571054935 * k1 + -6.359448489975075 * k2 + 4.3622954328695815 * k3), t + 0.9 * dt)
    k5 = _rhs(prob, u + dt * (5.325864828439257 * k1 + -11.748883564062828 * k2 + 7.4955393428898365 * k3 + -0.09249506636175525 * k4), t + 0.9800255409045097 * dt)
    k6 = _rhs(prob, u + dt * (5.86145544294642 * k1 + -12.92096931784711 * k2 + 8.159367898576159 * k3 + -0.071584973281401 * k4 + -0.028269050394068383 * k5), t + dt)
    unew = u + dt * (0.09646076681806523 * k1 + 0.01 * k2 + 0.4798896504144996 * k3 + 1.379008574103742 * k4 + -3.290069515436081 * k5 + 2.324710524099774 * k6)
    k7 = _rhs(prob, unew, t + dt)
    err = dt * (-0.00178001105222577714 * k1 + -0.0008164344596567469 * k2 + 0.007880878010261995 * k3 + -0.1447110071732629 * k4 + 0.5823571654525552 * k5 + -0.45808210592918697 * k6 + 0.015151515151515152 * k7)
    return unew, k7, err
end

function _tsit5_trial!(prob::ODEProblem, u, t, dt, k1, k2, k3, k4, k5, k6, k7, tmp, unew, err)
    _state_add_scaled!(tmp, u, dt * 0.161, k1)
    _rhs!(k2, prob, tmp, t + 0.161 * dt)
    _state_add2!(tmp, u, dt, -0.008480655492356989, k1, 0.335480655492357, k2)
    _rhs!(k3, prob, tmp, t + 0.327 * dt)
    _state_add3!(tmp, u, dt, 2.8971530571054935, k1, -6.359448489975075, k2, 4.3622954328695815, k3)
    _rhs!(k4, prob, tmp, t + 0.9 * dt)
    _state_add4!(tmp, u, dt, 5.325864828439257, k1, -11.748883564062828, k2, 7.4955393428898365, k3, -0.09249506636175525, k4)
    _rhs!(k5, prob, tmp, t + 0.9800255409045097 * dt)
    _state_add5!(tmp, u, dt, 5.86145544294642, k1, -12.92096931784711, k2, 8.159367898576159, k3, -0.071584973281401, k4, -0.028269050394068383, k5)
    _rhs!(k6, prob, tmp, t + dt)
    _state_add6!(unew, u, dt, 0.09646076681806523, k1, 0.01, k2, 0.4798896504144996, k3, 1.379008574103742, k4, -3.290069515436081, k5, 2.324710524099774, k6)
    _rhs!(k7, prob, unew, t + dt)
    _state_combo7!(err, dt, -0.00178001105222577714, k1, -0.0008164344596567469, k2, 0.007880878010261995, k3, -0.1447110071732629, k4, 0.5823571654525552, k5, -0.45808210592918697, k6, 0.015151515151515152, k7)
    return unew, k7, err
end

function _tsit5_error_norm(err::AbstractVector, uprev::AbstractVector, unew::AbstractVector, abstol, reltol)
    maxerr = 0.0
    for i in 1:length(err)
        scale = abstol + reltol * max(abs(uprev[i]), abs(unew[i]))
        e = abs(err[i]) / scale
        if e > maxerr
            maxerr = e
        end
    end
    return maxerr
end

function _tsit5_error_norm(err, uprev, unew, abstol, reltol)
    scale = abstol + reltol * max(abs(uprev), abs(unew))
    return abs(err) / scale
end

# Inline error norm that works for both scalars and static arrays (avoids
# CallTypedDispatch resolving to an AbstractVector method for immutable
# types like SVector; Issue #7984).
function _tsit5_error_norm_vec(err, uprev, unew, abstol, reltol)
    maxerr = 0.0
    for i in 1:length(err)
        scale = abstol + reltol * max(abs(uprev[i]), abs(unew[i]))
        e = abs(err[i]) / scale
        if e > maxerr
            maxerr = e
        end
    end
    return maxerr
end

function _tsit5_error_norm_array(err, uprev, unew, abstol, reltol)
    maxerr = 0.0
    for i in 1:length(err)
        scale = abstol + reltol * max(abs(uprev[i]), abs(unew[i]))
        e = abs(err[i]) / scale
        if e > maxerr
            maxerr = e
        end
    end
    return maxerr
end

function _tsit5_error_norm(err::AbstractArray, uprev::AbstractArray, unew::AbstractArray, abstol, reltol)
    return _tsit5_error_norm_array(err, uprev, unew, abstol, reltol)
end

function _tsit5_dt_factor(err_norm, accepted)
    if err_norm == 0
        return accepted ? 5.0 : 0.5
    end
    factor = 0.9 * err_norm^(-0.2)
    if accepted
        if factor < 0.2
            return 0.2
        elseif factor > 5.0
            return 5.0
        end
        return factor
    end
    if factor < 0.1
        return 0.1
    elseif factor > 0.5
        return 0.5
    end
    return factor
end

function _tsit5_solve_interval(prob::ODEProblem, u, t, target, h, k1, reltol, abstol, stats)
    # The buffered fast path reuses `k2…k7` via `_rhs!`, which only fills the
    # output buffer for an IN-PLACE RHS. An out-of-place RHS returns a fresh array
    # that the buffered trial ignores, so out-of-place states must take the generic
    # (operator-arithmetic) path (Issue #8163).
    if prob.isinplace && u isa AbstractVector && ismutable(u)
        return _tsit5_solve_interval_inplace(prob, u, t, target, h, k1, reltol, abstol, stats)
    end
    while t < target
        if h > target - t
            h = target - t
        end
        h > 0 || error("dt must be positive")

        unew, k7, err = _tsit5_trial(prob, u, t, h, k1)
        stats[:attempts] = stats[:attempts] + 1
        stats[:rhs_evals] = stats[:rhs_evals] + 6
        err_norm = _tsit5_error_norm_vec(err, u, unew, abstol, reltol)

        if err_norm <= 1.0
            t = t + h
            u = unew
            k1 = k7
            stats[:steps] = stats[:steps] + 1
            h = h * _tsit5_dt_factor(err_norm, true)
        else
            stats[:rejected_steps] = stats[:rejected_steps] + 1
            h = h * _tsit5_dt_factor(err_norm, false)
        end
    end
    return u, t, h, k1
end

function _tsit5_solve_interval_buffered(prob::ODEProblem, u, t, target, h, k1, reltol, abstol, stats, k2, k3, k4, k5, k6, k7, tmp, unew, err)
    while t < target
        if h > target - t
            h = target - t
        end
        h > 0 || error("dt must be positive")

        unew, k7, err = _tsit5_trial!(prob, u, t, h, k1, k2, k3, k4, k5, k6, k7, tmp, unew, err)
        stats[:attempts] = stats[:attempts] + 1
        stats[:rhs_evals] = stats[:rhs_evals] + 6
        err_norm = _tsit5_error_norm(err, u, unew, abstol, reltol)

        if err_norm <= 1.0
            t = t + h
            for i in 1:length(u)
                u[i] = unew[i]
            end
            for i in 1:length(k1)
                k1[i] = k7[i]
            end
            stats[:steps] = stats[:steps] + 1
            h = h * _tsit5_dt_factor(err_norm, true)
        else
            stats[:rejected_steps] = stats[:rejected_steps] + 1
            h = h * _tsit5_dt_factor(err_norm, false)
        end
    end
    return u, t, h, k1
end

function _tsit5_solve_interval_inplace(prob::ODEProblem, u, t, target, h, k1, reltol, abstol, stats)
    k2 = copy(u)
    k3 = copy(u)
    k4 = copy(u)
    k5 = copy(u)
    k6 = copy(u)
    k7 = copy(u)
    tmp = copy(u)
    unew = copy(u)
    err = copy(u)
    return _tsit5_solve_interval_buffered(prob, u, t, target, h, k1, reltol, abstol, stats, k2, k3, k4, k5, k6, k7, tmp, unew, err)
end

function _step_grid(t0, t1, dt)
    dt > 0 || error("dt must be positive")
    t1 >= t0 || error("MVP ODE solve only supports increasing tspan")

    n = Int64(ceil((t1 - t0) / dt))
    ts = [t0]
    for i in 1:n
        t = t0 + i * dt
        if t > t1
            t = t1
        end
        push!(ts, t)
    end
    if ts[end] != t1
        push!(ts, t1)
    end
    return ts
end

function _solve_grid(t0, t1, dt, saveat)
    if saveat === nothing
        step = dt === nothing ? (t1 - t0) / 1000 : dt
        return _step_grid(t0, t1, step)
    elseif saveat isa Number
        return _step_grid(t0, t1, saveat)
    else
        return collect(saveat)
    end
end

# Merge user `tstops` into the output grid so a step lands exactly on each
# requested time (Issue #7981). `tstops` strictly inside `(t0, t1)` are inserted
# (and saved — the MVP simplification: tstops become save points), then the grid
# is sorted and de-duplicated. `nothing` / empty tstops leave the grid unchanged.
function _merge_tstops(ts, tstops, t0, t1)
    if tstops === nothing
        return ts
    end
    merged = Float64[]
    for x in ts
        push!(merged, Float64(x))
    end
    for x in tstops
        xf = Float64(x)
        if xf > t0 && xf < t1
            push!(merged, xf)
        end
    end
    sorted = sort(merged)
    out = Float64[]
    for x in sorted
        if isempty(out) || x != out[end]
            push!(out, x)
        end
    end
    return out
end

# The Tsit5 algorithm token lives here, next to its `_tsit5_solve` stepper, so
# `solve(::ODEProblem, ::Tsit5)` can be a method ON `SciMLBase.solve`. The
# OrdinaryDiffEq facade re-exports this type (sjulia cannot extend SciMLBase's
# `solve` from another module — Issue #8052; PR #8050 review). Field surface
# mirrors upstream OrdinaryDiffEq's Tsit5 (stage_limiter, step_limiter, thread).
struct Tsit5
    stage_limiter
    step_limiter
    thread
    Tsit5() = new(nothing, nothing, :serial)
end

function _tsit5_solve(prob::ODEProblem, alg; dt=nothing, saveat=nothing, reltol=nothing, abstol=nothing, callback=nothing, tstops=nothing, kwargs...)
    t0 = prob.tspan[1]
    t1 = prob.tspan[2]
    if callback !== nothing
        h = dt === nothing ? (t1 - t0) / 1000 : dt
        return _solve_with_callbacks(prob, alg, _callbacks(callback), h, t0, t1)
    end
    ts = _merge_tstops(_solve_grid(t0, t1, dt, saveat), tstops, t0, t1)
    reltol = reltol === nothing ? 1e-3 : reltol
    abstol = abstol === nothing ? 1e-6 : abstol
    u = _copy_state(_densify_state(prob.u0))
    t = t0
    h = dt === nothing ? (length(ts) > 1 ? ts[2] - ts[1] : t1 - t0) : dt
    k1 = _rhs(prob, u, t)
    us = Any[]
    push!(us, _copy_state(u))
    stats = Dict(:algorithm => :Tsit5, :steps => 0, :attempts => 0, :rejected_steps => 0, :rhs_evals => 1)

    # Buffered fast path only for IN-PLACE RHS (the reusable buffers are filled by
    # `_rhs!`, which no-ops on the buffer for an out-of-place RHS) — Issue #8163.
    if prob.isinplace && u isa AbstractVector && ismutable(u)
        k2 = copy(u); k3 = copy(u); k4 = copy(u); k5 = copy(u)
        k6 = copy(u); k7 = copy(u); tmp = copy(u)
        unew_buf = copy(u); err_buf = copy(u)
        for i in 2:length(ts)
            u, t, h, k1 = _tsit5_solve_interval_buffered(prob, u, t, ts[i], h, k1, reltol, abstol, stats, k2, k3, k4, k5, k6, k7, tmp, unew_buf, err_buf)
            push!(us, _copy_state(u))
        end
    else
        for i in 2:length(ts)
            u, t, h, k1 = _tsit5_solve_interval(prob, u, t, ts[i], h, k1, reltol, abstol, stats)
            push!(us, _copy_state(u))
        end
    end

    return ODESolution(us, ts, prob, alg; stats=stats, retcode=ReturnCode.Success)
end

# Dispatch on alg type (Issue #7996): only the registered Tsit5 algorithm runs
# the stepper. Because this method is ON `SciMLBase.solve`, both the unqualified
# `solve(prob, Tsit5())` (via the OrdinaryDiffEq forwarder) and the qualified
# `SciMLBase.solve(prob, Tsit5())` dispatch here (Issue #8050 review).
function solve(prob::ODEProblem, alg::Tsit5; kwargs...)
    return _tsit5_solve(prob, alg; kwargs...)
end

function solve(prob::ODEProblem, alg; dt=nothing, saveat=nothing, reltol=nothing, abstol=nothing, callback=nothing, kwargs...)
    error("Algorithm $(typeof(alg)) is not supported yet. Use Tsit5().")
end

function solve(args...; kwargs...)
    error("No SciMLBase.solve method for these OrdinaryDiffEq README MVP arguments (Issue #7363)")
end

# ── Integrator interface subset (Issue #7981) ───────────────────────────────
# A minimal, README-adjacent subset of the SciML integrator interface built on
# top of the existing adaptive Tsit5 stepper. `init` builds an integrator,
# `step!(integ)` advances it to the next output (saveat) point, and
# `solve!(integ)` runs to the end, reproducing `solve(prob, alg; ...)`.
# Out of scope for this issue (tracked under #7981): the `step!(integ, dt,
# stop_at_tdt)` advance-by-dt form, user-supplied `tstops`, and a full
# `step!(integ, dt, stop_at_tdt)` and user `tstops` are supported below.

export init, step!, solve!, reinit!, remake, successful_retcode

# `successful_retcode` mirrors upstream `SciMLBase.successful_retcode`. A single
# method with runtime `isa` branches (rather than `::ReturnCode.T` / `::Symbol` /
# `::ODESolution` dispatched methods) avoids the specialization-dependent
# mis-dispatch seen in #8158. The `Symbol` arm keeps `:Success` parity for any
# pre-#7981 code that still compares to the old symbol retcode.
function successful_retcode(retcode)
    if retcode isa ODESolution
        return successful_retcode(retcode.retcode)
    elseif retcode isa ReturnCodeValue
        return retcode === ReturnCode.Success || retcode === ReturnCode.Terminated
    elseif retcode isa Symbol
        return retcode === :Success || retcode === :Terminated
    end
    return false
end

# `remake(prob; ...)` returns a new `ODEProblem` with the given fields overridden,
# re-deriving `isinplace` from the (possibly new) `f` / `u0` / `p`. `nothing`
# sentinels keep each unspecified field at its previous value.
function remake(prob::ODEProblem; f=nothing, u0=nothing, tspan=nothing,
                p=nothing, kwargs...)
    f = f === nothing ? prob.f : f
    u0 = u0 === nothing ? prob.u0 : u0
    tspan = tspan === nothing ? prob.tspan : tspan
    p = p === nothing ? prob.p : p
    new_kwargs = isempty(kwargs) ? prob.kwargs : kwargs
    return ODEProblem(f, u0, tspan, p, new_kwargs, _ode_isinplace(f, u0, tspan, p))
end

mutable struct ODEIntegrator
    prob
    alg
    u
    t
    dt          # current internal step size (h)
    k1          # cached f(u, t) for FSAL reuse across output steps
    ts          # output (saveat) grid
    reltol
    abstol
    sol_u       # accumulated saved states
    sol_t       # accumulated saved times (prefix of `ts`)
    stats
    save_idx    # index into `ts` of the next output point to reach (starts at 2)
    retcode
end

function init(prob::ODEProblem, alg; dt=nothing, saveat=nothing,
              reltol=nothing, abstol=nothing, tstops=nothing, kwargs...)
    t0 = prob.tspan[1]
    t1 = prob.tspan[2]
    ts = _merge_tstops(_solve_grid(t0, t1, dt, saveat), tstops, t0, t1)
    reltol = reltol === nothing ? 1e-3 : reltol
    abstol = abstol === nothing ? 1e-6 : abstol
    u = _copy_state(_densify_state(prob.u0))
    t = t0
    h = dt === nothing ? (length(ts) > 1 ? ts[2] - ts[1] : t1 - t0) : dt
    k1 = _rhs(prob, u, t)
    sol_u = Any[]
    push!(sol_u, _copy_state(u))
    sol_t = Any[t0]
    stats = Dict(:algorithm => :Tsit5, :steps => 0, :attempts => 0,
                 :rejected_steps => 0, :rhs_evals => 1)
    return ODEIntegrator(prob, alg, u, t, h, k1, ts, reltol, abstol,
                         sol_u, sol_t, stats, 2, ReturnCode.Default)
end

# Advance the integrator to the next output point. Returns `true` if a step was
# taken, `false` once the integrator has reached the end of the output grid.
function step!(integ::ODEIntegrator)
    if integ.save_idx > length(integ.ts)
        return false
    end
    target = integ.ts[integ.save_idx]
    u, t, h, k1 = _tsit5_solve_interval(integ.prob, integ.u, integ.t, target,
                                        integ.dt, integ.k1, integ.reltol,
                                        integ.abstol, integ.stats)
    integ.u = u
    integ.t = t
    integ.dt = h
    integ.k1 = k1
    push!(integ.sol_u, _copy_state(u))
    push!(integ.sol_t, t)
    integ.save_idx = integ.save_idx + 1
    return true
end

# Advance the integrator by `dt` rather than to the next output point (Issue
# #7981). Mirrors upstream `step!(integ, dt, stop_at_tdt)`: it integrates from the
# current `integ.t` to `integ.t + dt` and saves the reached state. `stop_at_tdt`
# is accepted for signature parity; this MVP always lands exactly on `t + dt`
# (the adaptive interval stepper caps its internal step at the target).
function step!(integ::ODEIntegrator, dt, stop_at_tdt=true)
    dt > 0 || error("step! dt must be positive")
    target = integ.t + dt
    u, t, h, k1 = _tsit5_solve_interval(integ.prob, integ.u, integ.t, target,
                                        integ.dt, integ.k1, integ.reltol,
                                        integ.abstol, integ.stats)
    integ.u = u
    integ.t = t
    integ.dt = h
    integ.k1 = k1
    push!(integ.sol_u, _copy_state(u))
    push!(integ.sol_t, t)
    return integ
end

function solve!(integ::ODEIntegrator)
    while step!(integ)
    end
    integ.retcode = ReturnCode.Success
    return ODESolution(integ.sol_u, integ.sol_t, integ.prob, integ.alg;
                       stats=integ.stats, retcode=ReturnCode.Success)
end

# Reset an integrator to a fresh initial state so it can be solved again.
# Explicit 1- and 2-positional methods (rather than one optional-positional +
# keyword method) to avoid the sjulia reduced-arity-vs-keyword interaction
# (Issue #7992): a generated `reinit!(integ; t0=...)` would drop the keyword.
reinit!(integ::ODEIntegrator; t0=nothing) =
    _reinit_impl!(integ, integ.prob.u0, t0)

reinit!(integ::ODEIntegrator, u0; t0=nothing) =
    _reinit_impl!(integ, u0, t0)

function _reinit_impl!(integ::ODEIntegrator, u0, t0)
    t0 = t0 === nothing ? integ.prob.tspan[1] : t0
    integ.u = _copy_state(u0)
    integ.t = t0
    integ.k1 = _rhs(integ.prob, integ.u, t0)
    integ.dt = length(integ.ts) > 1 ? integ.ts[2] - integ.ts[1] :
               integ.prob.tspan[2] - t0
    integ.sol_u = Any[]
    push!(integ.sol_u, _copy_state(integ.u))
    integ.sol_t = Any[t0]
    integ.stats = Dict(:algorithm => :Tsit5, :steps => 0, :attempts => 0,
                       :rejected_steps => 0, :rhs_evals => 1)
    integ.save_idx = 2
    integ.retcode = ReturnCode.Default
    return integ
end

# ── Dense output / continuous interpolation (Issue #7982) ───────────────────
# Make `ODESolution` callable so users can sample the trajectory at arbitrary
# times: `sol(t)`, `sol(t; idxs=...)`, and `sol(ts::AbstractVector)`. This MVP
# uses LINEAR interpolation between saved grid points, which is honest but NOT
# the 4th-order Tsit5 dense interpolant (tracked as a Phase B follow-up under
# #7982). `t` outside `tspan` clamps to the first/last saved state.

# Linear interpolation of the saved state at time `t`. Works for scalar states
# and vector states alike via the generic `u0 + theta * (u1 - u0)` form.
function _interp_state(sol::ODESolution, t)
    ts = sol.t
    us = sol.u
    n = length(ts)
    if t <= ts[1]
        return us[1]
    elseif t >= ts[n]
        return us[n]
    end
    i = 1
    while i < n && ts[i + 1] < t
        i += 1
    end
    t0 = ts[i]
    t1 = ts[i + 1]
    u0 = us[i]
    u1 = us[i + 1]
    theta = (t - t0) / (t1 - t0)
    return u0 + theta * (u1 - u0)
end

# Component selection mirroring `sol(t; idxs=...)`: `nothing` keeps the full
# state, an integer selects one component, any other index collection collects
# the selected components.
_select_idxs(u, idxs::Nothing) = u
_select_idxs(u, idxs::Integer) = u[idxs]
_select_idxs(u, idxs) = [u[i] for i in idxs]

(sol::ODESolution)(t::Number; idxs=nothing) = _select_idxs(_interp_state(sol, t), idxs)
(sol::ODESolution)(ts::AbstractVector; idxs=nothing) =
    [_select_idxs(_interp_state(sol, t), idxs) for t in ts]

# ── Second-order ODE problems + symplectic solver (Issue #7985) ─────────────
# Minimal `SecondOrderODEProblem` (u'' = f) plus a velocity-Verlet symplectic
# integrator, enough to run refined ODE / oscillator examples from the
# OrdinaryDiffEq README. The RHS computes the acceleration:
#   out-of-place: f(du, u, p, t) -> ddu
#   in-place:     f(ddu, du, u, p, t)
# Each saved state is the combined `[du...; u...]` vector (velocities then
# positions), matching upstream's DynamicalODE `[v; u]` ordering. A full
# `ArrayPartition` / dense interpolation surface stays out of scope (#7985).

abstract type AbstractSecondOrderODEProblem <: AbstractODEProblem end

struct SecondOrderODEProblem <: AbstractSecondOrderODEProblem
    f
    du0
    u0
    tspan
    p
    kwargs
    isinplace
end

function _so_isinplace(f, du0, u0, tspan, p)
    t0 = tspan[1]
    return hasmethod(f, Tuple{typeof(u0), typeof(du0), typeof(u0), typeof(p), typeof(t0)})
end

function SecondOrderODEProblem(f, du0, u0, tspan, p=NullParameters(); kwargs...)
    return SecondOrderODEProblem(f, du0, u0, tspan, p, kwargs,
                                 _so_isinplace(f, du0, u0, tspan, p))
end

# Acceleration ddu given velocity v and position u at time t.
function _accel(prob::SecondOrderODEProblem, v, u, t)
    if prob.isinplace
        ddu = _zero_like(u)
        prob.f(ddu, v, u, prob.p, t)
        return ddu
    end
    return prob.f(v, u, prob.p, t)
end

# Combined output state: velocities first, then positions (upstream `[v; u]`).
_combine_state(v::AbstractVector, u::AbstractVector) = vcat(v, u)
_combine_state(v, u) = [v, u]

# Velocity-Verlet integration over [t, target] with fixed internal step `h`
# (capped so it lands on `target`). Exact for position-only forces such as the
# harmonic oscillator; velocity-dependent forces use the explicit approximation.
function _verlet_interval(prob::SecondOrderODEProblem, v, u, t, target, h)
    while t < target
        step = h
        if step > target - t
            step = target - t
        end
        a0 = _accel(prob, v, u, t)
        unew = _state_add_scaled(u, step, v)
        unew = _state_add_scaled(unew, 0.5 * step * step, a0)
        a1 = _accel(prob, v, unew, t + step)
        v = _state_add_scaled(v, 0.5 * step, a0 + a1)
        u = unew
        t = t + step
    end
    return v, u, t
end

function solve(prob::SecondOrderODEProblem, alg; dt=nothing, saveat=nothing, kwargs...)
    t0 = prob.tspan[1]
    t1 = prob.tspan[2]
    ts = _solve_grid(t0, t1, dt, saveat)
    h = dt === nothing ? (length(ts) > 1 ? ts[2] - ts[1] : t1 - t0) : dt
    u = _copy_state(prob.u0)
    v = _copy_state(prob.du0)
    t = t0
    us = Any[]
    push!(us, _combine_state(v, u))
    stats = Dict(:algorithm => :VelocityVerlet, :steps => 0)
    for i in 2:length(ts)
        v, u, t = _verlet_interval(prob, v, u, t, ts[i], h)
        push!(us, _combine_state(v, u))
        stats[:steps] = stats[:steps] + 1
    end
    return ODESolution(us, ts, prob, alg; stats=stats, retcode=ReturnCode.Success)
end

# ── Callbacks & events (Issue #7983) ────────────────────────────────────────
# A minimal callback subset for first-order ODEProblems: DiscreteCallback,
# ContinuousCallback (with bisection root-finding), and CallbackSet, wired
# through `solve(prob, alg; callback=...)`. The callback path uses fixed-step
# RK4 and saves every internal step (so `saveat` is the `dt` grid). Out of scope
# for this issue (tracked under #7983): VectorContinuousCallback, save_positions
# control, and callbacks on the adaptive Tsit5 / integrator interface paths.

export DiscreteCallback, ContinuousCallback, CallbackSet

# condition(u, t, integrator) -> Bool; affect!(integrator) mutates integrator.u.
struct DiscreteCallback
    condition
    affect!
end

# condition(u, t, integrator) -> Real; an event is a sign change of condition.
# affect!(integrator) mutates integrator.u at the located event time.
struct ContinuousCallback
    condition
    affect!
end

struct CallbackSet
    callbacks
end

CallbackSet(cbs...) = CallbackSet(cbs)

# Normalize a `callback=` argument to a tuple of individual callbacks.
_callbacks(cb::CallbackSet) = cb.callbacks
_callbacks(cb) = (cb,)

# Lightweight integrator handed to callback `affect!` / `condition` closures.
mutable struct CallbackIntegrator
    u
    t
    p
end

# One classic RK4 step of the first-order RHS.
function _rk4_step(prob, u, t, h)
    k1 = _rhs(prob, u, t)
    k2 = _rhs(prob, _state_add_scaled(u, 0.5 * h, k1), t + 0.5 * h)
    k3 = _rhs(prob, _state_add_scaled(u, 0.5 * h, k2), t + 0.5 * h)
    k4 = _rhs(prob, _state_add_scaled(u, h, k3), t + h)
    return _state_add4(u, h / 6.0, 1.0, k1, 2.0, k2, 2.0, k3, 1.0, k4)
end

# Bisect [t0, t1] for the event time where `condition` crosses zero, using a
# linear interpolation of the state across the (small) step.
function _bisect_event(c, u0, t0, u1, t1, integ)
    lo = t0
    hi = t1
    glo = c.condition(u0, t0, integ)
    tmid = t1
    umid = u1
    for _ in 1:60
        tmid = 0.5 * (lo + hi)
        theta = (tmid - t0) / (t1 - t0)
        umid = u0 + theta * (u1 - u0)
        gmid = c.condition(umid, tmid, integ)
        if glo * gmid <= 0.0
            hi = tmid
        else
            lo = tmid
            glo = gmid
        end
    end
    return tmid, umid
end

function _solve_with_callbacks(prob, alg, cbs, h, t0, t1)
    u = _copy_state(prob.u0)
    t = t0
    integ = CallbackIntegrator(u, t, prob.p)
    ts = Any[t0]
    us = Any[_copy_state(u)]
    nsteps = 0
    maxsteps = 1000000
    skip_continuous = false
    while t < t1 && nsteps < maxsteps
        nsteps = nsteps + 1
        step = h
        if step > t1 - t
            step = t1 - t
        end
        unew = _rk4_step(prob, u, t, step)
        tnew = t + step
        event = false
        for c in cbs
            if c isa ContinuousCallback
                if !skip_continuous
                    g0 = c.condition(u, t, integ)
                    g1 = c.condition(unew, tnew, integ)
                    if g0 * g1 < 0.0
                        te, ue = _bisect_event(c, u, t, unew, tnew, integ)
                        integ.u = ue
                        integ.t = te
                        c.affect!(integ)
                        u = integ.u
                        t = te
                        push!(ts, t)
                        push!(us, _copy_state(u))
                        event = true
                        break
                    end
                end
            elseif c isa DiscreteCallback
                if c.condition(unew, tnew, integ)
                    integ.u = unew
                    integ.t = tnew
                    c.affect!(integ)
                    unew = integ.u
                end
            end
        end
        # Skip continuous-event detection on the step right after an event so we
        # don't re-trigger on the same near-zero crossing.
        skip_continuous = event
        if !event
            u = unew
            t = tnew
            push!(ts, t)
            push!(us, _copy_state(u))
        end
    end
    stats = Dict(:algorithm => :Tsit5, :steps => nsteps)
    return ODESolution(us, ts, prob, alg; stats=stats, retcode=ReturnCode.Success)
end

end # module SciMLBase
