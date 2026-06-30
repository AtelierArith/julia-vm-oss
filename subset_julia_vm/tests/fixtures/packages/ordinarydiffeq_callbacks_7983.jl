# OrdinaryDiffEq callbacks & events (Issue #7983): ContinuousCallback (with
# bisection root-finding), DiscreteCallback, and CallbackSet wired through
# solve(prob, alg; callback=...). Canonical example: the bouncing ball.
#
# affect!/condition are NAMED functions: an anonymous function whose body is an
# index assignment (`integ -> (integ.u[2] = ...)`) does not lower in sjulia yet
# (Issue #8007). Filtered comprehensions are kept on one line: a newline before
# the `if` guard does not parse yet (Issue #8008).

using Test
using OrdinaryDiffEq

# bouncing ball: u = [height, velocity], u' = [velocity, -g]
ballrhs(u, p, t) = [u[2], -9.81]
ballheight(u, t, integ) = u[1]
function bounce!(integ)
    integ.u[2] = -0.8 * integ.u[2]
    return nothing
end

# step counter: u' = 0, increment u[1] on every step via a DiscreteCallback
zerorhs(u, p, t) = [0.0]
always(u, t, integ) = true
function increment!(integ)
    integ.u[1] = integ.u[1] + 1.0
    return nothing
end

# combined ball + counter: u = [height, velocity, counter], counter is inert in
# the RHS and only advanced by the discrete callback (component 3), so the two
# callbacks in the CallbackSet never touch the same component.
ballcount_rhs(u, p, t) = [u[2], -9.81, 0.0]
function count3!(integ)
    integ.u[3] = integ.u[3] + 1.0
    return nothing
end

@testset "ContinuousCallback bouncing ball" begin
    prob = ODEProblem(ballrhs, [1.0, 0.0], (0.0, 3.0))
    cb = ContinuousCallback(ballheight, bounce!)
    sol = solve(prob, Tsit5(); dt=0.01, callback=cb)

    @test successful_retcode(sol)

    heights = [s[1] for s in sol.u]
    # the ball never sinks through the floor
    @test minimum(heights) > -1e-6
    # it never rises above its release height
    @test maximum(heights) <= 1.0 + 1e-9

    # bounce times = velocity sign flips from downward to upward
    bounce_ts = [sol.t[i] for i in 2:length(sol.t) if sol.u[i - 1][2] < 0 && sol.u[i][2] > 0]
    @test length(bounce_ts) >= 3
    # first bounce at sqrt(2*h0/g) = sqrt(2/9.81) ≈ 0.4515 s
    @test abs(bounce_ts[1] - 0.4515) < 0.01
    # damping ⇒ later bounce intervals shrink
    @test (bounce_ts[2] - bounce_ts[1]) > (bounce_ts[3] - bounce_ts[2])
end

@testset "DiscreteCallback fires every step" begin
    prob = ODEProblem(zerorhs, [0.0], (0.0, 1.0))
    cb = DiscreteCallback(always, increment!)
    sol = solve(prob, Tsit5(); dt=0.25, callback=cb)

    @test successful_retcode(sol)
    # u' = 0 so the only change is the per-step increment
    @test sol.u[end][1] == sol.stats[:steps]
    @test sol.u[end][1] >= 4
end

@testset "CallbackSet combines continuous + discrete" begin
    # state [height, velocity, counter]; bouncer touches u[1]/u[2], counter u[3]
    prob = ODEProblem(ballcount_rhs, [1.0, 0.0, 0.0], (0.0, 1.0))
    bouncer = ContinuousCallback(ballheight, bounce!)
    counter = DiscreteCallback(always, count3!)
    cbs = CallbackSet(bouncer, counter)
    sol = solve(prob, Tsit5(); dt=0.01, callback=cbs)

    @test successful_retcode(sol)
    heights = [s[1] for s in sol.u]
    @test minimum(heights) > -1e-6
    # the ball still bounces (continuous callback)
    bounce_ts = [sol.t[i] for i in 2:length(sol.t) if sol.u[i - 1][2] < 0 && sol.u[i][2] > 0]
    @test length(bounce_ts) >= 1
    # the counter advanced (discrete callback)
    @test sol.u[end][3] > 0.0
end

# Regression gate (Issue #8158): the fixture harness only checks the script's
# FINAL value against `expected = true`, and sjulia `@test` failures print but do
# NOT throw — so the `@testset`s above cannot catch a regression on their own.
# This block recomputes the critical outcomes and ends the script with a boolean
# that is `false` if a CallbackSet silently stops firing its callbacks (the
# #8158 `_callbacks(::CallbackSet)` mis-dispatch did exactly that).
function _callbacks_regression_gate()
    prob = ODEProblem(ballcount_rhs, [1.0, 0.0, 0.0], (0.0, 1.0))
    cbs = CallbackSet(ContinuousCallback(ballheight, bounce!),
                      DiscreteCallback(always, count3!))
    sol = solve(prob, Tsit5(); dt=0.01, callback=cbs)
    # BOTH callbacks in the set must have fired:
    counter_fired = sol.u[end][3] > 0.0                          # DiscreteCallback
    heights = [s[1] for s in sol.u]
    ball_bounced = minimum(heights) > -1e-6                      # ContinuousCallback
    # An individually-passed ContinuousCallback must still fire too:
    prob2 = ODEProblem(ballrhs, [1.0, 0.0], (0.0, 3.0))
    sol2 = solve(prob2, Tsit5(); dt=0.01,
                 callback=ContinuousCallback(ballheight, bounce!))
    single_bounced = minimum([s[1] for s in sol2.u]) > -1e-6
    return counter_fired && ball_bounced && single_bounced
end

_callbacks_regression_gate()
