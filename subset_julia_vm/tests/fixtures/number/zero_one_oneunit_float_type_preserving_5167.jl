# Regression: zero/one/oneunit must be TYPE-PRESERVING for the float
# subtypes Float16/Float32/Float64 (Issue #5167, follow-up to #5076).
#
# Before the fix, base/number.jl defined `zero`/`one` only for
# Int64/Float64/BigInt/Int32/Int16/Int8/Bool — no Float32/Float16. So:
#   one(2.0f0)        -> NoMethodFound (Dispatch error)  (upstream: 1.0f0::Float32)
#   one(Float16(1))   -> NoMethodFound (Dispatch error)  (upstream: 1.0::Float16)
#   f(x)=one(x); f(2.0f0) -> 1::Int64 (widened)          (upstream: 1.0f0::Float32)
# `zero(2.0f0)` happened to work via the native fallback, but the explicit
# methods are now present for both DIRECT and untyped calls.
#
# Use `===` / typeof to catch the TYPE, not just the value (1 == 1.0 is true).

using Test

@testset "zero float subtypes" begin
    @test zero(Float16(1)) === Float16(0)
    @test zero(2.0f0) === 0.0f0
    @test zero(2.0) === 0.0
    @test typeof(zero(Float16(1))) === Float16
    @test typeof(zero(2.0f0)) === Float32
    @test typeof(zero(2.0)) === Float64
end

@testset "one float subtypes" begin
    @test one(Float16(1)) === Float16(1)
    @test one(2.0f0) === 1.0f0
    @test one(2.0) === 1.0
    @test typeof(one(Float16(1))) === Float16
    @test typeof(one(2.0f0)) === Float32
    @test typeof(one(2.0)) === Float64
end

@testset "oneunit float subtypes" begin
    @test oneunit(Float16(1)) === Float16(1)
    @test oneunit(2.0f0) === 1.0f0
    @test oneunit(2.0) === 1.0
    @test typeof(oneunit(Float16(1))) === Float16
    @test typeof(oneunit(2.0f0)) === Float32
    @test typeof(oneunit(2.0)) === Float64
end

# Untyped forwarding through a user function must preserve the concrete
# runtime type (this is the type-generic-call case from the issue).
fzero(x) = zero(x)
fone(x) = one(x)
foneunit(x) = oneunit(x)

@testset "untyped forwarding preserves float type" begin
    @test fone(2.0f0) === 1.0f0
    @test fone(Float16(1)) === Float16(1)
    @test fone(2.0) === 1.0
    @test fzero(2.0f0) === 0.0f0
    @test fzero(Float16(1)) === Float16(0)
    @test fzero(2.0) === 0.0
    @test foneunit(2.0f0) === 1.0f0
    @test foneunit(Float16(1)) === Float16(1)
    @test foneunit(2.0) === 1.0
end

true
