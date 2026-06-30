# OrdinaryDiffEq SecondOrderODEProblem + velocity-Verlet symplectic integrator
# (Issue #7985). Saved state is the combined [du...; u...] vector (velocities
# then positions). Verified on the harmonic oscillator (analytic cos/-sin) and a
# 2D decoupled system (bounded energy).

using Test
using OrdinaryDiffEq

const TWO_PI = 2 * pi

@testset "SecondOrderODEProblem harmonic oscillator (scalar)" begin
    # u'' = -u, u(0)=1, u'(0)=0  ->  u(t)=cos(t), u'(t)=-sin(t)
    harmonic(du, u, p, t) = -u
    prob = SecondOrderODEProblem(harmonic, 0.0, 1.0, (0.0, TWO_PI))
    @test prob.u0 == 1.0
    @test prob.du0 == 0.0
    @test prob.isinplace == false

    sol = solve(prob, VelocityVerlet(); dt=0.001, saveat=0.1)
    @test successful_retcode(sol)

    # combined state [du, u]
    @test length(sol.u[1]) == 2
    @test sol.u[1] == [0.0, 1.0]

    final = sol.u[end]
    vfin = final[1]   # velocity du
    ufin = final[2]   # position u
    @test abs(ufin - cos(TWO_PI)) < 1e-2
    @test abs(vfin - (-sin(TWO_PI))) < 1e-2

    # energy 0.5*du^2 + 0.5*u^2 stays ~0.5 (symplectic)
    E0 = 0.5 * sol.u[1][1]^2 + 0.5 * sol.u[1][2]^2
    Eend = 0.5 * vfin^2 + 0.5 * ufin^2
    @test abs(E0 - 0.5) < 1e-12
    @test abs(Eend - 0.5) < 1e-3
end

@testset "SecondOrderODEProblem 2D decoupled (bounded energy)" begin
    # u1'' = -u1, u2'' = -4 u2
    decoupled(du, u, p, t) = [-u[1], -4.0 * u[2]]
    prob = SecondOrderODEProblem(decoupled, [0.0, 1.0], [1.0, 0.0], (0.0, 1.0))
    sol = solve(prob, VelocityVerlet(); dt=0.001, saveat=0.1)
    @test length(sol.t) == 11

    # combined state [du1, du2, u1, u2]
    @test length(sol.u[1]) == 4
    @test sol.u[1] == [0.0, 1.0, 1.0, 0.0]

    final = sol.u[end]
    # H = 0.5*(du1^2 + du2^2) + 0.5*(u1^2 + 4 u2^2)
    E0 = 0.5 * (sol.u[1][1]^2 + sol.u[1][2]^2) +
         0.5 * (sol.u[1][3]^2 + 4.0 * sol.u[1][4]^2)
    Eend = 0.5 * (final[1]^2 + final[2]^2) +
           0.5 * (final[3]^2 + 4.0 * final[4]^2)
    @test abs(E0 - 1.0) < 1e-12
    @test abs(Eend - E0) < 1e-2
end

# Regression gate: end with a boolean computed from the actual solution so a
# regression flips the script's final value to `false` (the fixture harness only
# checks the final value, and sjulia `@test` failures print but do not throw).
function _secondorder_gate()
    harmonic(du, u, p, t) = -u
    prob = SecondOrderODEProblem(harmonic, 0.0, 1.0, (0.0, TWO_PI))
    sol = solve(prob, VelocityVerlet(); dt=0.001, saveat=0.1)
    ufin = sol.u[end][2]
    vfin = sol.u[end][1]
    analytic = abs(ufin - cos(TWO_PI)) < 1e-2 && abs(vfin - (-sin(TWO_PI))) < 1e-2
    energy = abs(0.5 * vfin^2 + 0.5 * ufin^2 - 0.5) < 1e-3   # symplectic: bounded
    combined_order = sol.u[1] == [0.0, 1.0]                  # [du, u] ordering
    return analytic && energy && combined_order
end

_secondorder_gate()
