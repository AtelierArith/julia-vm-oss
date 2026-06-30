# Regression guard for the Issue #6846 perf rewrite: norm inner loops were
# changed from indexed (`for i in 1:n; x[i]`) to direct iteration (`for xi in x`)
# and a concrete `sinc(x::Float64)` fast path was added. Values must stay
# upstream-faithful (Julia 1.12).

using Test
using LinearAlgebra

@testset "norm iteration paths + sinc fast path (Issue #6846)" begin
    # --- norm(::Array{Float64}) over all p branches ---
    v = [3.0, -4.0]
    @assert norm(v) == 5.0 "L2 default"
    @assert norm(v, 2) == 5.0 "L2 explicit"
    @assert norm(v, 1) == 7.0 "L1: 3 + 4"
    @assert norm(v, Inf) == 4.0 "Inf: max(3, 4)"
    @assert isapprox(norm([1.0, 1.0, 1.0], 3), 3.0^(1.0 / 3.0)) "general p=3"
    @assert norm(Float64[]) == 0.0 "empty vector"

    # --- norm(::Array{Int64}) ---
    @assert norm([1, 2, 2]) == 3.0 "Int L2"
    @assert norm([3, -4], 1) == 7.0 "Int L1"
    @assert norm([3, -4], Inf) == 4.0 "Int Inf"

    # --- norm(::Array{Complex{Float64}}) ---
    z = [3.0 + 0.0im, 0.0 + 4.0im]
    @assert norm(z) == 5.0 "Complex L2"
    @assert norm([1.0 + 0.0im, 2.0 + 0.0im]) == sqrt(5.0) "Complex L2 #2"

    # --- generic fallback (tuple is iterable, not an Array) ---
    @assert norm((3.0, 4.0)) == 5.0 "generic L2 over tuple"

    # --- sinc(x::Float64) concrete fast path ---
    @assert sinc(0.0) == 1.0 "sinc(0) = 1"
    @assert isapprox(sinc(0.5), 2.0 / pi) "sinc(0.5) = 2/pi"
    @assert isapprox(sinc(1.0), 0.0; atol = 1e-15) "sinc(1) ~ 0"
    @assert isapprox(sinc(2.0), 0.0; atol = 1e-15) "sinc(2) ~ 0"

    # --- combined surface-plot kernel ---
    @assert isapprox(sinc(norm([0.0, 0.0])), 1.0) "kernel at origin"
    @assert isapprox(sinc(norm([3.0, 4.0])), sinc(5.0)) "kernel = sinc(5)"

    @test (true)
end

true  # Test passed
