# Aizawa attractor — AoT acceptance fixture (Issue #8639).
#
# One of the three ADR_BACKEND_STRATEGY.md acceptance programs (coprime pi /
# Aizawa / Mandelbrot): the AoT backend must compile AND run this program with
# output identical to upstream Julia. Same kernel as
# benchmarks/aizawa_attractor_bench_for.jl (explicit-Euler integration, pure
# Float64 scalar hot loop written with a `for` loop, (x+y+z) checksum) with a
# fixed small n and no timing so stdout is deterministic.
#
# Expected output (julia 1.12.6, n=10000): 6617.642224697513

function aizawa(n::Int64)::Float64
    a = 0.95; b = 0.7; c = 0.6; d = 3.5; e = 0.25; g = 0.1
    dt = 0.01
    x = 0.1; y = 0.0; z = 0.0
    sx = 0.0; sy = 0.0; sz = 0.0
    for _ in 1:n
        dx = (z - b) * x - d * y
        dy = d * x + (z - b) * y
        dz = c + a * z - z * z * z / 3.0 - (x * x + y * y) * (1.0 + e * z) + g * z * x * x * x
        x = x + dx * dt
        y = y + dy * dt
        z = z + dz * dt
        sx = sx + x; sy = sy + y; sz = sz + z
    end
    sx + sy + sz
end

result = aizawa(10000)
println(result)
result == 6617.642224697513
