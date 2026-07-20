# Correctness companion for Float64-array × Int-scalar broadcasting.
#
# Issue #7587: `x .^ 2` (and `.+ 2`, `.* 2`, …) on a `Vector{Float64}` used to
# be ~8-14x slower than the same operation with a Float64 scalar (`x .^ 2.0`),
# because the per-element mixed-type call `^(::Float64, ::Int)` fell through to
# the generic `^(::Number, ::Number)` promote() fallback in promotion.jl
# instead of a concrete fast method. Re-adding concrete mixed Float/Int
# arithmetic methods (base/float.jl) closes the gap.
#
# This fixture used to assert wall-clock ratios (Int-scalar form vs
# Float64-scalar form). Wall-clock ratio @tests in the shared fixture suite are
# structurally load-sensitive: a sibling broadcast perf fixture flaked under
# full-suite saturation once the #9360 @testset gate made fixture @test
# failures gating (Issue #9489). Per repo policy (Issue #3210) the performance
# guard lives in a Criterion benchmark with stable statistical methodology:
#
#     cargo bench -p subset_julia_vm --bench vm_broadcast_mixed_float_int_benchmark
#
# Here we keep only the load-robust correctness checks: each Int-scalar
# broadcast must agree with its scalar-loop expansion, and (for + and *, where
# Int promotion is exact) with its Float64-scalar twin.

using Test

@testset "Float64-array × Int-scalar broadcast correctness (#7587)" begin
    n = 500
    x = [Float64(i) * 0.01 for i in 1:n]

    pow_int = x .^ 2
    pow_flt = x .^ 2.0
    add_int = x .+ 2
    add_flt = x .+ 2.0
    mul_int = x .* 2
    mul_flt = x .* 2.0

    # Elementwise agreement with the scalar-call expansion.
    @test pow_int == [x[i]^2 for i in 1:n]
    @test add_int == [x[i] + 2 for i in 1:n]
    @test mul_int == [x[i] * 2 for i in 1:n]

    # + and * promote the Int scalar exactly to Float64, so the Int-scalar and
    # Float64-scalar forms must match exactly.
    @test add_int == add_flt
    @test mul_int == mul_flt

    # ^ may take different code paths for Int vs Float64 exponents
    # (literal_pow vs pow); require elementwise approximate agreement.
    @test all(isapprox(pow_int[i], pow_flt[i]) for i in 1:n)

    @test eltype(pow_int) == Float64
    @test eltype(add_int) == Float64
    @test eltype(mul_int) == Float64
end

true
