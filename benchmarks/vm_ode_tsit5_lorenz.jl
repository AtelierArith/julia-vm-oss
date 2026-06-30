# VM benchmark source for the OrdinaryDiffEq Lorenz Tsit5 solve (Issue #8094).
# Isolates the adaptive in-place-buffered Tsit5 stepper (≈96% of the iOS Lorenz
# sample wall time) from Plots artifact generation. Prints the saved-point count
# so the harness can assert a deterministic result.
using OrdinaryDiffEq

function lorenz!(du, u, p, t)
    du[1] = 10.0 * (u[2] - u[1])
    du[2] = u[1] * (28.0 - u[3]) - u[2]
    du[3] = u[1] * u[2] - (8 / 3) * u[3]
end

u0 = [1.0, 0.0, 0.0]
prob = ODEProblem(lorenz!, u0, (0.0, 20.0))
sol = solve(prob, Tsit5(), dt=0.02, saveat=0.02)
println(length(sol.t))
