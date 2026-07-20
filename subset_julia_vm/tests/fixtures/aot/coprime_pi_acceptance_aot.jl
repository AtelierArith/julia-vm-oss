# Coprime-probability π estimate — AoT acceptance fixture (Issue #8639).
#
# One of the three ADR_BACKEND_STRATEGY.md acceptance programs (coprime pi /
# Aizawa / Mandelbrot): the AoT backend must compile AND run this program with
# output identical to upstream Julia. Derived from benchmarks/calc_pi_aot.jl
# with a fixed small N and no timing so stdout is deterministic.
#
# P(gcd(a,b) = 1) = 6/π² → π ≈ √(6/P)
# Expected output (julia 1.12.6, N=100): 3.139597498005517

function mygcd(a::Int64, b::Int64)::Int64
    while b != 0
        tmp = b
        b = a % b
        a = tmp
    end
    a
end

function calc_pi(N::Int64)::Float64
    cnt = 0
    for a in 1:N
        for b in 1:N
            if mygcd(a, b) == 1
                cnt += 1
            end
        end
    end
    prob = cnt / N / N
    sqrt(6.0 / prob)
end

result = calc_pi(100)
println(result)
result == 3.139597498005517
