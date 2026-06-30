# OrdinaryDiffEq integrator interface — ReturnCode, tstops, step!(integ, dt)
# (Issue #7981). `sol.retcode` is now a real `ReturnCode` value (not the `:Success`
# symbol), `successful_retcode` branches on it, user `tstops` make a step land on
# requested times, and `step!(integ, dt, stop_at_tdt)` advances by `dt`.

using Test
using OrdinaryDiffEq

f(u, p, t) = 1.01 * u
prob = ODEProblem(f, 0.5, (0.0, 1.0))

@testset "retcode is a real ReturnCode value" begin
    sol = solve(prob, Tsit5(); dt=0.1, saveat=0.1)
    # a real return-code value, not the MVP :Success symbol
    @test sol.retcode === ReturnCode.Success
    @test sol.retcode isa SciMLBase.ReturnCodeValue
    @test !(sol.retcode isa Symbol)
    # successful_retcode works on the solution and on the value
    @test successful_retcode(sol)
    @test successful_retcode(sol.retcode)
    @test successful_retcode(ReturnCode.Success)
    @test successful_retcode(ReturnCode.Terminated)
    @test !successful_retcode(ReturnCode.Failure)
    @test !successful_retcode(ReturnCode.MaxIters)
    # round-trips through the namespace identity
    @test ReturnCode.Success === ReturnCode.Success
    @test ReturnCode.Success !== ReturnCode.Failure
    # :Success symbol parity retained for pre-#7981 code
    @test successful_retcode(:Success)
    @test !successful_retcode(:Failure)
end

@testset "tstops make a step land on requested times" begin
    sol = solve(prob, Tsit5(); dt=0.1, saveat=0.1, tstops=[0.55, 0.77])
    @test 0.55 in sol.t
    @test 0.77 in sol.t
    # the analytic solution still holds at a tstop
    i = findfirst(==(0.55), sol.t)
    @test abs(sol.u[i] - 0.5 * exp(1.01 * 0.55)) < 0.02
end

@testset "step!(integ, dt, stop_at_tdt) advances by dt" begin
    integ = init(prob, Tsit5(); dt=0.1, saveat=0.1)
    step!(integ, 0.25, true)
    @test abs(integ.t - 0.25) < 1e-12
    step!(integ, 0.25, true)
    @test abs(integ.t - 0.5) < 1e-12
    @test abs(integ.u - 0.5 * exp(1.01 * 0.5)) < 0.02
end

# Regression gate.
function _retcode_gate()
    sol = solve(prob, Tsit5(); dt=0.1, saveat=0.1)
    retcode_ok = sol.retcode === ReturnCode.Success && successful_retcode(sol) &&
                 !successful_retcode(ReturnCode.Failure)
    solts = solve(prob, Tsit5(); dt=0.1, saveat=0.1, tstops=[0.55])
    tstops_ok = 0.55 in solts.t
    integ = init(prob, Tsit5(); dt=0.1, saveat=0.1)
    step!(integ, 0.25, true)
    step_ok = abs(integ.t - 0.25) < 1e-12
    return retcode_ok && tstops_ok && step_ok
end

_retcode_gate()
