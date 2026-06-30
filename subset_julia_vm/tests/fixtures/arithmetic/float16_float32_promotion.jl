# Issue #3750: Float16 + Float32 must yield Float32, not Float64.
# Julia's promotion lattice has Float32 dominate Float16, so any binary op
# between F16 and F32 produces F32.
#
# The bug only appeared when the operation flowed through generic dispatch:
# inside an untyped function `f(a, b) = a + b` the body is compiled with
# `Any+Any` and routes through CallDynamicBinaryBoth. F16+F32 then dispatches
# into +(::Number, ::Number) where promote(F16, F32) = (F32, F32) and the
# recursive `px + py` reaches the runtime primitive fallback. The fallback
# was widening F32+F32 to F64+F64 before invoking AddFloat, dropping the
# result to Float64. The fix: include both_f32 in the F32 path of
# `vm/exec/binary_both.rs::execute_binary_both` so F32+F32 produces F32.

using Test

@testset "Float16 + Float32 promotion (top-level)" begin
    @test typeof(Float16(1.0) + Float32(1.0)) === Float32
    @test typeof(Float32(1.0) + Float16(1.0)) === Float32
    @test typeof(Float16(1.0) - Float32(1.0)) === Float32
    @test typeof(Float32(1.0) - Float16(1.0)) === Float32
    @test typeof(Float16(1.0) * Float32(1.0)) === Float32
    @test typeof(Float32(1.0) * Float16(1.0)) === Float32
    @test typeof(Float16(1.0) / Float32(1.0)) === Float32
    @test typeof(Float32(1.0) / Float16(1.0)) === Float32
end

@testset "Float16 + Float32 in untyped generic function (Issue #3750)" begin
    fadd(a, b) = a + b
    fsub(a, b) = a - b
    fmul(a, b) = a * b
    fdiv(a, b) = a / b

    @test typeof(fadd(Float16(1.0), Float32(1.0))) === Float32
    @test typeof(fadd(Float32(1.0), Float16(1.0))) === Float32
    @test typeof(fsub(Float16(1.0), Float32(1.0))) === Float32
    @test typeof(fsub(Float32(1.0), Float16(1.0))) === Float32
    @test typeof(fmul(Float16(1.0), Float32(1.0))) === Float32
    @test typeof(fmul(Float32(1.0), Float16(1.0))) === Float32
    @test typeof(fdiv(Float16(1.0), Float32(1.0))) === Float32
    @test typeof(fdiv(Float32(1.0), Float16(1.0))) === Float32
end

@testset "Float16 + Float32 in typed-parameter function" begin
    g1(a::Float16, b::Float32) = a + b
    g2(a::Float32, b::Float16) = a + b
    g3(a::Float16, b::Float32) = a * b
    g4(a::Float16, b::Float32) = a / b

    @test typeof(g1(Float16(1.0), Float32(2.0))) === Float32
    @test typeof(g2(Float32(2.0), Float16(1.0))) === Float32
    @test typeof(g3(Float16(2.0), Float32(3.0))) === Float32
    @test typeof(g4(Float16(6.0), Float32(2.0))) === Float32
end

@testset "Float32+Float32 must remain Float32 (regression for #3750 fix)" begin
    # The fix added both_f32 to the F32 path. Make sure same-type F32 ops
    # (which can reach the runtime fallback via +(::Number, ::Number) on
    # F16+F32 promotion) still produce Float32.
    fadd(a, b) = a + b
    fsub(a, b) = a - b
    fmul(a, b) = a * b
    fdiv(a, b) = a / b

    @test typeof(fadd(Float32(1.0), Float32(2.0))) === Float32
    @test typeof(fsub(Float32(1.0), Float32(2.0))) === Float32
    @test typeof(fmul(Float32(1.0), Float32(2.0))) === Float32
    @test typeof(fdiv(Float32(1.0), Float32(2.0))) === Float32
end

@testset "Float32+Float64 must remain Float64 (regression for #3750 fix)" begin
    # F32+F64 must still pick F64 — Float64 dominates Float32.
    fadd(a, b) = a + b
    fmul(a, b) = a * b

    @test typeof(fadd(Float32(1.0), Float64(1.0))) === Float64
    @test typeof(fadd(Float64(1.0), Float32(1.0))) === Float64
    @test typeof(fmul(Float32(2.0), Float64(3.0))) === Float64
    @test typeof(fmul(Float64(3.0), Float32(2.0))) === Float64
end

@testset "Float16 + Float32 numeric value sanity" begin
    fadd(a, b) = a + b
    fsub(a, b) = a - b
    fmul(a, b) = a * b
    fdiv(a, b) = a / b

    @test fadd(Float16(2.0), Float32(3.0)) ≈ 5.0f0
    @test fsub(Float16(7.0), Float32(2.0)) ≈ 5.0f0
    @test fmul(Float16(2.0), Float32(3.0)) ≈ 6.0f0
    @test fdiv(Float16(6.0), Float32(2.0)) ≈ 3.0f0
end

@testset "Float16 + Float32 comparisons return Bool" begin
    # Comparisons should be unaffected by the promotion fix.
    fcmp_eq(a, b) = a == b
    fcmp_lt(a, b) = a < b
    fcmp_le(a, b) = a <= b
    fcmp_gt(a, b) = a > b
    fcmp_ge(a, b) = a >= b
    fcmp_ne(a, b) = a != b

    @test typeof(fcmp_eq(Float16(1.0), Float32(1.0))) === Bool
    @test typeof(fcmp_lt(Float16(1.0), Float32(2.0))) === Bool
    @test fcmp_eq(Float16(1.0), Float32(1.0))
    @test fcmp_lt(Float16(1.0), Float32(2.0))
    @test fcmp_le(Float16(1.0), Float32(1.0))
    @test fcmp_gt(Float16(2.0), Float32(1.0))
    @test fcmp_ge(Float16(1.0), Float32(1.0))
    @test fcmp_ne(Float16(1.0), Float32(2.0))
end

true
