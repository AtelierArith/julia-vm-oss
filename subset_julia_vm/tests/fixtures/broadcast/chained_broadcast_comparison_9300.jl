# Regression test for chained broadcast (dotted) comparisons (Issue #9300).
#
# `a .<= v .< b` is a chained comparison whose operators are broadcast. Upstream
# Julia lowers it through the broadcast-fused `&` path (`expand-vector-compare`),
# i.e. `(a .<= v) .& (v .< b)`, NOT the scalar short-circuit `&&`. The old
# lowering did not recognize dotted comparison operators as chainable, so it
# evaluated the chain left-associatively as `(a .<= v) .< b` — comparing a Bool
# array against the scalar bound — collapsing the whole result to all-`false`.
#
# This fixture also guards the scalar chain (`0 <= x < 1`) against regressions
# and checks that a non-atomic middle operand is evaluated exactly once.

using Test

@testset "Chained broadcast comparison (Float64)" begin
    v = [0.4, 0.3, 0.9]
    @test (0 .<= v .< 1) == Bool[1, 1, 1]
    # Manual expansion is the ground truth.
    @test (0 .<= v .< 1) == ((0 .<= v) .& (v .< 1))

    w = [-0.5, 0.5, 1.5]
    @test (0 .<= w .< 1) == Bool[0, 1, 0]
    @test (0 .<= w .< 1) == ((0 .<= w) .& (w .< 1))
end

@testset "Chained broadcast comparison (Int)" begin
    vi = [0, 3, -1]
    @test (0 .<= vi .< 5) == Bool[1, 1, 0]
    @test (0 .<= vi .< 5) == ((0 .<= vi) .& (vi .< 5))
end

@testset "Chained broadcast comparison (Float32 / Float16)" begin
    v32 = Float32[0.4, 0.3, 0.9]
    @test (0 .<= v32 .< 1) == Bool[1, 1, 1]

    v16 = Float16[0.5, 1.5, 2.5]
    @test (0 .<= v16 .< 1) == Bool[1, 0, 0]

    # Negative Float16 element: the lower-bound `.<=` used to flip to true
    # (Issue #9348, fixed; see float16_le_ge_negative_9348.jl).
    v16n = Float16[-0.5, 0.5, 1.5]
    @test (0 .<= v16n .< 1) == Bool[0, 1, 0]
end

@testset "Chained broadcast comparison (.== and .>= .>)" begin
    @test (1 .== [1, 2, 3] .== 1) == Bool[1, 0, 0]
    x = [2, 5, 8]
    @test (10 .>= x .> 3) == Bool[0, 1, 1]
end

@testset "Three-operand dotted chain and 4-operand chain" begin
    a = [2, 1, 4]
    @test (0 .< a .< 3 .< 10) == Bool[1, 1, 0]
end

@testset "Mixed scalar + dotted chain" begin
    # Leading scalar comparison is false -> whole broadcast is false.
    @test (5 < 3 .<= [2, 3, 4]) == Bool[0, 0, 0]
    # Leading scalar comparison is true -> tail broadcast decides.
    @test (1 < 3 .<= [2, 3, 4]) == Bool[0, 1, 1]
    # Dotted first, scalar tail.
    @test ([1, 3, 5] .< 4 < 100) == Bool[1, 1, 0]
end

@testset "Scalar chained comparison regression" begin
    @test (0 <= 5 < 10) == true
    @test (0 <= 50 < 10) == false
    @test (1 < 2 < 3 < 4) == true
    @test (1 < 2 < 5 < 4) == false
end

@testset "Non-atomic middle operand evaluated once" begin
    count = Ref(0)
    g = function ()
        count[] += 1
        [0.4, 0.3, 0.9]
    end
    r = 0 .<= g() .< 1
    @test r == Bool[1, 1, 1]
    @test count[] == 1
end

true
