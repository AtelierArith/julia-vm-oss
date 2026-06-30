# OrdinaryDiffEq broader SciML array surfaces (Issue #7986): view-backed
# (`SubArray`) states flow through the Tsit5 stepper and produce the same
# trajectory as the dense-`Vector` version, without mutating the view's backing
# buffer. A `SubArray` u0 is densified to a fresh dense `Vector` for integration
# (matching upstream, which copies u0 into dense internal storage). sparse states
# are densified by the same rule, but the bundled SparseArrays subset does not yet
# implement `sparse`/`sparsevec`, so a sparse state cannot reach the solver — that
# is the documented densify decision (no sparse case is exercised here).
#
# Also pins the out-of-place VECTOR RHS path that the #8094 buffered fast path
# regressed (Issue #8163): the buffered path is now gated on `prob.isinplace`.

using Test
using OrdinaryDiffEq

# in-place linear system: u1' = -u1, u2' = -2 u2  ->  [exp(-t), exp(-2t)]
function lin!(du, u, p, t)
    du[1] = -u[1]
    du[2] = -2.0 * u[2]
end
# same system, out-of-place (returns a fresh Vector)
lin(u, p, t) = [-u[1], -2.0 * u[2]]

tspan = (0.0, 1.0)
ref = solve(ODEProblem(lin!, [1.0, 1.0], tspan), Tsit5(); dt=0.1, saveat=0.1)

function _max_end_diff(a, b)
    m = 0.0
    for i in 1:length(a)
        d = abs(a[i] - b[i])
        if d > m
            m = d
        end
    end
    return m
end

@testset "view-backed state (in-place RHS)" begin
    # u0 is a view into the MIDDLE of a larger buffer (9.9 sentinels on the ends)
    buffer = [9.9, 1.0, 1.0, 9.9]
    u0 = view(buffer, 2:3)
    @test u0 isa SubArray
    sol = solve(ODEProblem(lin!, u0, tspan), Tsit5(); dt=0.1, saveat=0.1)
    # matches the dense-Vector reference exactly (same stepper, densified copy)
    @test _max_end_diff(sol.u[end], ref.u[end]) < 1e-12
    # the saved state is a dense Vector, not a SubArray
    @test sol.u[end] isa Vector
    # the view's backing buffer is NOT mutated by the solve (sentinels intact)
    @test buffer[1] == 9.9
    @test buffer[4] == 9.9
end

@testset "view-backed state (out-of-place RHS)" begin
    buffer = [0.0, 1.0, 1.0, 0.0]
    u0 = view(buffer, 2:3)
    sol = solve(ODEProblem(lin, u0, tspan), Tsit5(); dt=0.1, saveat=0.1)
    @test _max_end_diff(sol.u[end], ref.u[end]) < 1e-12
end

@testset "view-backed state through the integrator interface" begin
    buffer = [7.0, 1.0, 1.0, 7.0]
    u0 = view(buffer, 2:3)
    integ = init(ODEProblem(lin!, u0, tspan), Tsit5(); dt=0.1, saveat=0.1)
    while step!(integ)
    end
    sol = solve!(integ)
    @test _max_end_diff(sol.u[end], ref.u[end]) < 1e-12
    @test buffer[1] == 7.0   # backing buffer untouched
end

@testset "out-of-place vector RHS correctness (Issue #8163 regression)" begin
    # Plain dense Vector u0 with an out-of-place vector RHS: the buffered fast path
    # must NOT be used (it ignored out-of-place results). Expect the analytic answer.
    sol = solve(ODEProblem(lin, [1.0, 1.0], tspan), Tsit5(); dt=0.1, saveat=0.1)
    @test _max_end_diff(sol.u[end], ref.u[end]) < 1e-12
    @test abs(sol.u[end][1] - exp(-1.0)) < 1e-3
    @test abs(sol.u[end][2] - exp(-2.0)) < 1e-3
end

# Regression gate: end with a boolean computed from the actual solutions so a
# regression flips the script's final value to `false` (the fixture harness only
# checks the final value; sjulia `@test` failures print but do not throw).
function _view_state_gate()
    buffer = [9.9, 1.0, 1.0, 9.9]
    sol_v = solve(ODEProblem(lin!, view(buffer, 2:3), tspan), Tsit5(); dt=0.1, saveat=0.1)
    view_ok = _max_end_diff(sol_v.u[end], ref.u[end]) < 1e-12 &&
              buffer[1] == 9.9 && buffer[4] == 9.9
    sol_oop = solve(ODEProblem(lin, [1.0, 1.0], tspan), Tsit5(); dt=0.1, saveat=0.1)
    oop_ok = abs(sol_oop.u[end][1] - exp(-1.0)) < 1e-3 &&
             abs(sol_oop.u[end][2] - exp(-2.0)) < 1e-3
    return view_ok && oop_ok
end

_view_state_gate()
