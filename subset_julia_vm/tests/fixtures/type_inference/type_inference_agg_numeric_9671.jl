# Aggregated concat-safe @testset fixtures (Issue #9671 Phase 3 expansion).
# Each block below is one former standalone fixture, verbatim except its
# `using Test` / trailing `true` were hoisted. @testset names (with their
# original Issue numbers) are preserved, and the #9360 @testset gate still
# detects any per-@testset failure. Source fixture in each banner.
using Test

# ===== source: type_inference/abs_sign_type_preservation_8617.jl =====
# Regression fixture: general tfunc inference preserves return types for
# abs/abs2/sign across the full numeric type family without the name-keyed
# dispatch override (Issue #8617, parent #8608).
#
# The `dispatch_abs_sign` and `infer_abs_sign` override ids were removed
# in Issue #8617 after the audit confirmed the general tfunc registry
# (tfunc_abs, tfunc_sign, first-arg preservation in expr_tfuncs.rs) already
# covers all the same cases.  This fixture is the regression guard.


@testset "abs/abs2/sign type preservation" begin
    # Int64 (default)
    @test typeof(abs(-3)) == Int64
    @test typeof(abs2(-3)) == Int64
    @test typeof(sign(-42)) == Int64

    # Int32 / Int16 / Int8
    @test typeof(abs(Int32(-3))) == Int32
    @test typeof(abs(Int16(-3))) == Int16
    @test typeof(abs(Int8(-3))) == Int8
    @test typeof(abs2(Int32(-3))) == Int32
    @test typeof(sign(Int32(-42))) == Int32

    # Int128
    @test typeof(abs(Int128(-3))) == Int128
    @test typeof(abs2(Int128(-3))) == Int128
    @test typeof(sign(Int128(-42))) == Int128

    # UInt types (abs of unsigned is a no-op value-wise; type must be preserved)
    @test typeof(abs(UInt8(3))) == UInt8
    @test typeof(abs(UInt32(3))) == UInt32
    @test typeof(abs2(UInt8(3))) == UInt8

    # Float64
    @test typeof(abs(-3.5)) == Float64
    @test typeof(abs2(-3.5)) == Float64
    @test typeof(sign(-2.5)) == Float64

    # Float32
    @test typeof(abs(Float32(-3.5))) == Float32
    @test typeof(abs2(Float32(-3.5))) == Float32
    @test typeof(sign(Float32(-2.5))) == Float32

    # Float16
    @test typeof(abs(Float16(-3.5))) == Float16
    @test typeof(abs2(Float16(-2.5))) == Float16
    @test typeof(sign(Float16(-2.5))) == Float16

    # BigInt
    @test typeof(abs(big(-3))) == BigInt
    @test typeof(abs2(big(-3))) == BigInt
    @test typeof(sign(big(-42))) == BigInt

    # BigFloat
    @test typeof(abs(big(-3.5))) == BigFloat

    # Parametric context: where T inference
    function pabs(x::T) where T
        abs(x)
    end
    @test typeof(pabs(Int64(-7))) == Int64
    @test typeof(pabs(big(-7))) == BigInt
    @test typeof(pabs(Float32(-7.5))) == Float32
    @test typeof(pabs(Int128(-7))) == Int128
end

# ===== source: type_inference/arithmetic_bit_width.jl =====

# Test that arithmetic on same-type small integers preserves bit width (Issue #2278).
# Previously, Int8(1) + Int8(2) returned Int64 instead of Int8.

@testset "Signed integer arithmetic preserves bit width" begin
    @test typeof(Int8(1) + Int8(2)) == Int8
    @test typeof(Int8(3) - Int8(1)) == Int8
    @test typeof(Int8(2) * Int8(3)) == Int8
    @test typeof(Int16(1) + Int16(2)) == Int16
    @test typeof(Int16(3) - Int16(1)) == Int16
    @test typeof(Int16(2) * Int16(3)) == Int16
    @test typeof(Int32(1) + Int32(2)) == Int32
    @test typeof(Int32(3) - Int32(1)) == Int32
    @test typeof(Int32(2) * Int32(3)) == Int32
end

@testset "Unsigned integer arithmetic preserves bit width" begin
    @test typeof(UInt8(1) + UInt8(2)) == UInt8
    @test typeof(UInt8(3) - UInt8(1)) == UInt8
    @test typeof(UInt8(2) * UInt8(3)) == UInt8
    @test typeof(UInt16(1) + UInt16(2)) == UInt16
    @test typeof(UInt16(3) - UInt16(1)) == UInt16
    @test typeof(UInt16(2) * UInt16(3)) == UInt16
    @test typeof(UInt32(1) + UInt32(2)) == UInt32
    @test typeof(UInt32(3) - UInt32(1)) == UInt32
    @test typeof(UInt32(2) * UInt32(3)) == UInt32
    @test typeof(UInt64(1) + UInt64(2)) == UInt64
    @test typeof(UInt64(3) - UInt64(1)) == UInt64
    @test typeof(UInt64(2) * UInt64(3)) == UInt64
end

@testset "Float16/Float32 arithmetic preserves bit width" begin
    @test typeof(Float16(1.0) + Float16(2.0)) == Float16
    @test typeof(Float16(3.0) - Float16(1.0)) == Float16
    @test typeof(Float16(2.0) * Float16(3.0)) == Float16
    @test typeof(Float32(1.0) + Float32(2.0)) == Float32
    @test typeof(Float32(3.0) - Float32(1.0)) == Float32
    @test typeof(Float32(2.0) * Float32(3.0)) == Float32
end

# ===== source: type_inference/bool_arithmetic_widening.jl =====
# Test Bool arithmetic widens to Int64, not Bool (Issue #3462)


@testset "type_inference_bool_arithmetic_widening: Bool+Bool yields Int64" begin
    @test typeof(true + false) == Int64
    @test typeof(true + true) == Int64
    @test typeof(false + 1) == Int64
end

# ===== source: type_inference/complex_abs2_type.jl =====
# Test abs2 return type for Complex{Float64} (Issue #3466)
# Note: Complex{Float32} abs2 runtime return type is tracked separately


@testset "type_inference_complex_abs2_type: abs2(Complex{Float64}) returns Float64" begin
    z = Complex{Float64}(1.0, 2.0)
    @test typeof(z) == Complex{Float64}
    @test typeof(abs2(z)) == Float64
end

# ===== source: type_inference/divremmod_int64_fallback_9528.jl =====
# Regression fixture: the div/rem/mod inference fixed-fallback must be
# arg-type-dependent (Issue #9528).
#
# `compile/expr/infer/expr_tfuncs.rs` used to map `div`/`rem`/`mod` to an
# unconditional `Int64` fixed fallback. When the registry tfunc could not pin a
# result (e.g. mixed `Int64 x Float64` => Top), that fallback pinned `Int64`, so
# a call nested as an argument of a resolved typed call got a `DynamicToI64`
# coercion inserted on its Float64 result (-0.2 -> 0, -0.0 -> 0). Binding the
# result to a variable first, or feeding an untyped consumer, avoided the
# fallback, so the same expression gave different answers depending on syntactic
# context. Upstream: `mod(::Int64, ::Float64)::Float64`.


@testset "div/rem/mod int64 fallback (Issue #9528)" begin
    # Nested-call context: the float result must NOT be coerced to Int64.
    @test signbit(mod(Int64(5), -2.6)) == true
    @test signbit(mod(Int64(5), -2.5)) == true

    # Variable-bound context (already correct before the fix).
    x = mod(Int64(5), -2.6)
    @test signbit(x) == true

    # Untyped consumer (already correct before the fix).
    @test string(mod(Int64(5), -2.6)) == "-0.20000000000000018"

    # Nested vs variable-bound must agree.
    @test signbit(mod(Int64(5), -2.6)) == signbit(x)

    # Mixed args yield a Float64 result in every context.
    @test typeof(mod(Int64(5), -2.6)) == Float64
    @test typeof(rem(Int64(5), 2.5)) == Float64
    @test typeof(div(7, 2.0)) == Float64
    @test mod(Int64(5), -2.6) ≈ -0.20000000000000018

    # All-integer args still infer/behave as Int64 (fallback preserved).
    @test typeof(div(7, 2)) == Int64
    @test typeof(mod(10, 3)) == Int64
    @test typeof(rem(10, 3)) == Int64
    @test div(7, 2) == 3
    @test mod(10, 3) == 1
    @test signbit(mod(-7, 3)) == false
end

# ===== source: type_inference/float32_type_preservation.jl =====
# Test Float32 type preservation for arithmetic and math functions (Issue #3462)


@testset "type_inference_float32_preservation: Float32 preserved in arithmetic" begin
    x = Float32(2.0)
    y = Float32(3.0)

    # Division preserves Float32
    @test typeof(x / y) == Float32
    # Power preserves Float32 when exponent is not Float64
    @test typeof(x ^ 2) == Float32
    @test typeof(x ^ y) == Float32
    # Float64 dominates
    @test typeof(x / 1.0) == Float64
end

# ===== source: type_inference/gcd_complex_return_types_8618.jl =====
# Regression fixture: general tfunc inference preserves return types for
# gcd/lcm (BigInt preservation) and the Complex overloads of the Pure Julia
# math functions, without the name-keyed return-type overrides
# (Issue #8618, parent #8608).
#
# The last three overrides — `dispatch_gcd_lcm`, `infer_gcd_lcm`, and
# `dispatch_complex_math` (Issue #4341) — were removed in Issue #8618 after
# the audit (#8616) confirmed the general inference machinery already
# produces the same return types as upstream julia. This fixture is the
# regression guard.
#
# Scope note: gcd/lcm are only exercised for Int64 and BigInt because gcd/lcm
# on Int128 / unsigned widths is a pre-existing sjulia MethodError (Issue
# #8812), and only the forward Complex math functions are exercised because
# the inverse trig / hyperbolic Complex overloads are a pre-existing sjulia
# MethodError (Issue #8813). Both are unrelated to the removed overrides.


@testset "gcd/lcm BigInt preservation" begin
    @test typeof(gcd(12, 18)) == Int64
    @test typeof(lcm(4, 6)) == Int64
    @test gcd(12, 18) == 6
    @test lcm(4, 6) == 12

    @test typeof(gcd(big(12), big(18))) == BigInt
    @test typeof(lcm(big(4), big(6))) == BigInt
    @test gcd(big(12), big(18)) == big(6)
    @test lcm(big(4), big(6)) == big(12)

    # Parametric context: where T inference must not widen to Any.
    function pgcd(a::T, b::T) where T
        gcd(a, b)
    end
    @test typeof(pgcd(12, 18)) == Int64
    @test typeof(pgcd(big(12), big(18))) == BigInt
end

@testset "Complex math return types" begin
    z = 1.0 + 2.0im
    @test typeof(sqrt(z)) == ComplexF64
    @test typeof(sin(z)) == ComplexF64
    @test typeof(cos(z)) == ComplexF64
    @test typeof(tan(z)) == ComplexF64
    @test typeof(sinh(z)) == ComplexF64
    @test typeof(cosh(z)) == ComplexF64
    @test typeof(tanh(z)) == ComplexF64
    @test typeof(exp(z)) == ComplexF64
    @test typeof(log(z)) == ComplexF64

    # Magnitude reductions return the real Float64 (method return type path).
    @test typeof(abs(z)) == Float64
    @test typeof(abs2(z)) == Float64

    # Assigning the Complex result to a top-level binding must not emit a
    # StoreF64 (the original Issue #4341 failure mode).
    r = tan(z)
    @test typeof(r) == ComplexF64
end

# ===== source: type_inference/math_intrinsics_type_preservation.jl =====
# Test that math intrinsics preserve Float16/Float32 types (Issue #2221)
# In Julia, sqrt(Float32(4.0)) returns Float32, not Float64.


@testset "Math intrinsics type preservation" begin
    # Float32 preservation
    @test typeof(sqrt(Float32(4.0))) == Float32
    @test typeof(floor(Float32(3.7))) == Float32
    @test typeof(ceil(Float32(3.2))) == Float32
    @test typeof(trunc(Float32(3.9))) == Float32
    @test typeof(abs(Float32(-2.5))) == Float32
    @test typeof(abs2(Float32(-2.5))) == Float32
    @test typeof(sign(Float32(-2.5))) == Float32
    @test typeof(signbit(Float32(-2.5))) == Bool
    @test typeof(sin(Float32(0.5))) == Float32
    @test typeof(cos(Float32(0.5))) == Float32
    @test typeof(exp(Float32(1.0))) == Float32
    @test typeof(log(Float32(4.0))) == Float32

    # Float32 value correctness
    @test sqrt(Float32(4.0)) == Float32(2.0)
    @test floor(Float32(3.7)) == Float32(3.0)
    @test ceil(Float32(3.2)) == Float32(4.0)
    @test trunc(Float32(3.9)) == Float32(3.0)
    @test abs(Float32(-2.5)) == Float32(2.5)
    @test abs2(Float32(-2.5)) == Float32(6.25)
    @test sign(Float32(-2.5)) == Float32(-1.0)

    # Float16 preservation
    @test typeof(sqrt(Float16(4.0))) == Float16
    @test typeof(floor(Float16(3.5))) == Float16
    @test typeof(ceil(Float16(3.5))) == Float16
    @test typeof(trunc(Float16(3.5))) == Float16
    @test typeof(abs(Float16(-2.5))) == Float16
    @test typeof(abs2(Float16(-2.5))) == Float16
    @test typeof(sign(Float16(-2.5))) == Float16
    @test typeof(signbit(Float16(-2.5))) == Bool
    @test typeof(sin(Float16(0.5))) == Float16
    @test typeof(cos(Float16(0.5))) == Float16
    @test typeof(exp(Float16(1.0))) == Float16
    @test typeof(log(Float16(4.0))) == Float16

    # Integer and Bool sign/abs2 width preservation
    @test typeof(sign(Int8(-3))) == Int8
    @test typeof(sign(UInt8(3))) == UInt8
    @test typeof(sign(Int128(-3))) == Int128
    @test typeof(sign(true)) == Bool
    @test typeof(abs2(Int8(-3))) == Int8
    @test typeof(abs2(UInt8(3))) == UInt8
    @test typeof(signbit(Int8(-3))) == Bool
    @test typeof(signbit(UInt8(3))) == Bool
    @test typeof(signbit(true)) == Bool

    # Float64 still works
    @test typeof(sqrt(4.0)) == Float64
    @test typeof(floor(3.7)) == Float64
    @test typeof(ceil(3.2)) == Float64
    @test typeof(trunc(3.9)) == Float64
    @test typeof(abs(-2.5)) == Float64
    @test typeof(abs2(-2.5)) == Float64
    @test typeof(sign(-2.5)) == Float64
    @test typeof(signbit(-2.5)) == Bool
    @test typeof(sin(0.5)) == Float64
    @test typeof(cos(0.5)) == Float64
    @test typeof(exp(1.0)) == Float64
    @test typeof(log(4.0)) == Float64
    @test repr(sign(-0.0)) == "-0.0"
end

# ===== source: type_inference/min_max_promotion.jl =====
# Test that min/max return promoted type, not Union (Issue #3479)


@testset "type_inference_min_max_promotion: min/max use Julia promotion" begin
    @test typeof(max(1, 2.0)) == Float64
    @test typeof(min(1, 2.0)) == Float64
    @test typeof(max(Int8(1), Int16(2))) == Int16
    @test typeof(min(Int32(1), Int64(2))) == Int64
    @test typeof(max(Int32(2), Int64(1))) == Int64
    @test typeof(min(Int32(2), Int64(1))) == Int64
    @test typeof(max(1, 2)) == Int64
    @test typeof(min(Int8(1), Int8(2))) == Int8
    @test typeof(max(UInt8(1), UInt8(2))) == UInt8
    @test typeof(min(Float16(1), Float16(2))) == Float16
    @test typeof(max(Float32(1), Float32(2))) == Float32
    @test typeof(max(false, true)) == Bool
    lo, hi = minmax(Int32(2), Int64(1))
    @test typeof(lo) == Int64
    @test typeof(hi) == Int64
end

# ===== source: type_inference/numeric_bit_width.jl =====

# Test that numeric type constructors preserve bit width (Issue #1663).
# Previously, the type inference mapped smaller types (Int8/Int16/Int32, Float32)
# to larger types (Int64, Float64), losing type precision.

@testset "Integer constructor bit width" begin
    @test typeof(Int8(1)) == Int8
    @test typeof(Int16(1)) == Int16
    @test typeof(Int32(1)) == Int32
    @test typeof(Int64(1)) == Int64
end

@testset "Unsigned integer constructor bit width" begin
    @test typeof(UInt8(1)) == UInt8
    @test typeof(UInt16(1)) == UInt16
    @test typeof(UInt32(1)) == UInt32
    @test typeof(UInt64(1)) == UInt64
end

@testset "Float constructor bit width" begin
    @test typeof(Float16(1.0)) == Float16
    @test typeof(Float32(1.0)) == Float32
    @test typeof(Float64(1.0)) == Float64
end

@testset "Bool constructor" begin
    @test typeof(true) == Bool
    @test typeof(false) == Bool
end

# ===== source: type_inference/sum_prod_widening.jl =====
# Test that sum/prod apply Julia's reduction widening rules (Issue #3478)


@testset "type_inference_sum_prod_widening: sum/prod use widening rules" begin
    # Bool array -> Int64
    bools = [true, false, true]
    @test typeof(sum(bools)) == Int64
    # Int64 array -> Int64
    ints = [1, 2, 3]
    @test typeof(sum(ints)) == Int64
    @test typeof(prod(ints)) == Int64
    # Float64 array -> Float64
    floats = [1.0, 2.0, 3.0]
    @test typeof(sum(floats)) == Float64
end

# ===== source: type_inference/typemin_typemax_preservation_8617.jl =====
# Regression fixture: general tfunc inference preserves return types for
# typemin/typemax across all numeric types without the name-keyed dispatch
# override (Issue #8617, parent #8608).
#
# The `dispatch_typemin_typemax` and `infer_typemin_typemax` override ids were
# removed in Issue #8617 after the audit confirmed the shared tfunc registry
# (tfunc_typemin, tfunc_typemax, type-object call inference in expr_tfuncs.rs)
# already handles them.  This fixture is the regression guard.


@testset "typemin/typemax type preservation" begin
    # Signed integers
    @test typeof(typemin(Int64)) == Int64
    @test typeof(typemax(Int64)) == Int64
    @test typeof(typemin(Int32)) == Int32
    @test typeof(typemax(Int32)) == Int32
    @test typeof(typemin(Int16)) == Int16
    @test typeof(typemax(Int16)) == Int16
    @test typeof(typemin(Int8)) == Int8
    @test typeof(typemax(Int8)) == Int8
    @test typeof(typemin(Int128)) == Int128
    @test typeof(typemax(Int128)) == Int128

    # Unsigned integers
    @test typeof(typemin(UInt8)) == UInt8
    @test typeof(typemax(UInt8)) == UInt8
    @test typeof(typemin(UInt16)) == UInt16
    @test typeof(typemax(UInt16)) == UInt16
    @test typeof(typemin(UInt32)) == UInt32
    @test typeof(typemax(UInt32)) == UInt32
    @test typeof(typemin(UInt64)) == UInt64
    @test typeof(typemax(UInt64)) == UInt64
    @test typeof(typemin(UInt128)) == UInt128
    @test typeof(typemax(UInt128)) == UInt128

    # Floating point
    @test typeof(typemin(Float64)) == Float64
    @test typeof(typemax(Float64)) == Float64
    @test typeof(typemin(Float32)) == Float32
    @test typeof(typemax(Float32)) == Float32
    @test typeof(typemin(Float16)) == Float16
    @test typeof(typemax(Float16)) == Float16

    # Bool
    @test typeof(typemax(Bool)) == Bool
    @test typeof(typemin(Bool)) == Bool

    # Arithmetic on result exercises statically-inferred type
    @test typeof(typemax(Int32) - Int32(1)) == Int32
    @test typeof(typemin(Int8) + Int8(1)) == Int8
    @test typeof(typemax(UInt8) - UInt8(1)) == UInt8

    # Parametric context: where T inference
    function audit_ptm(::Type{T}) where T
        typemin(T)
    end
    @test typeof(audit_ptm(Int16)) == Int16
    @test typeof(audit_ptm(UInt32)) == UInt32
    @test typeof(audit_ptm(Float32)) == Float32
end

true
