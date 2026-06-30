# Correctness / type-parity for mixed Float/Int arithmetic (Issue #7587).
#
# Re-adding concrete mixed-type methods (e.g. +(::Float64, ::Integer)) for the
# no-JIT VM must preserve upstream Julia's values and result types exactly — the
# promote() fallback already produced these results, just slowly. These assertions
# lock that behavior so the perf optimization cannot silently change it.
# All expected values verified against upstream `julia` 1.12.

using Test

@testset "scalar Float64 × Int values and types (#7587)" begin
    @test 1.5 + 2 === 3.5
    @test 2 + 1.5 === 3.5
    @test 1.5 - 2 === -0.5
    @test 2 - 1.5 === 0.5
    @test 1.5 * 2 === 3.0
    @test 2 * 1.5 === 3.0
    @test 6.0 / 2 === 3.0
    @test 2.0^3 === 8.0
    @test (-2.0)^3 === -8.0
    @test (-2.0)^2 === 4.0
    @test 2.0^-1 === 0.5
    @test 2^1.5 === 2.8284271247461903   # Int base, Float64 exponent
    # Bool is an Integer subtype
    @test 1.5 + true === 2.5
    @test 2.0^true === 2.0
end

@testset "scalar Float32 × Int preserves Float32 (#7587)" begin
    @test 1.5f0 + 2 === 3.5f0
    @test 2 + 1.5f0 === 3.5f0
    @test 1.5f0 - 2 === -0.5f0
    @test 1.5f0 * 2 === 3.0f0
    @test 6.0f0 / 2 === 3.0f0
    @test 2.0f0^3 === 8.0f0
end

@testset "BigInt operands stay on the promote path, not intercepted (#7587)" begin
    # The fast methods are scoped to Int64; BigInt must NOT be intercepted and
    # keeps flowing through the generic promote() / AbstractFloat-power paths,
    # unchanged by this fix. Use == so this tolerates BigFloat-vs-Float64.
    @test 1.5 + big(2) == 3.5
    @test big(2) + 1.5 == 3.5
    @test 1.5 * big(2) == 3.0
    @test 2.0 ^ big(3) == 8.0   # AbstractFloat ^ BigInt (handled by #7609), not intercepted
end

@testset "Float64-array broadcast with Int scalar (#7587)" begin
    x = [1.0, 2.0, 3.0]
    @test x .^ 2 == [1.0, 4.0, 9.0]
    @test eltype(x .^ 2) == Float64
    @test x .+ 2 == [3.0, 4.0, 5.0]
    @test eltype(x .+ 2) == Float64
    @test x .- 2 == [-1.0, 0.0, 1.0]
    @test x .* 2 == [2.0, 4.0, 6.0]
    @test x ./ 2 == [0.5, 1.0, 1.5]
    # scalar on the left
    @test 2 .+ x == [3.0, 4.0, 5.0]
    @test 2 .* x == [2.0, 4.0, 6.0]
    @test 2 .^ x == [2.0, 4.0, 8.0]
    @test eltype(2 .^ x) == Float64
end

@testset "Float32-array broadcast with Int scalar preserves Float32 (#7587)" begin
    v = Float32[1, 2, 3]
    @test v .+ 2 == Float32[3, 4, 5]
    @test eltype(v .+ 2) == Float32
    @test v .^ 2 == Float32[1, 4, 9]
    @test eltype(v .^ 2) == Float32
    @test v .* 2 == Float32[2, 4, 6]
    @test eltype(v .* 2) == Float32
    @test v ./ 2 == Float32[0.5, 1.0, 1.5]
    @test eltype(v ./ 2) == Float32
end

@testset "Issue #7587 expression matches Float-exponent form" begin
    x = collect(-1.0:0.1:1.0)
    t = 0.3
    @test exp.(-(x .- t) .^ 2) == exp.(-(x .- t) .^ 2.0)
end

true
