using Test

# Issues #9304, #9316, #9318: the compiler's binary-op result-type inference
# hard-coded a Float64-family result for `^`, `*`, and for the `//` (rational)
# call when the operands are BigInt / BigFloat / Rational. The spurious Float64
# (or Int64) made an enclosing operation pick the wrong fast path or bind the
# wrong static method:
#
#   * #9304 — inline `big(2)//3` was inferred `BigFloat` (the abstract
#     `//(n::Integer, d::Integer) = Rational(promote(n, d)...)` return type),
#     so `BigFloat(1) + big(2)//3` compiled to `CallIntrinsic(AddBigFloat)`,
#     whose `pop_bigfloat` rejected the Rational `StructRef`
#     ("expected BigFloat, got StructRef").
#   * #9316 — `big(2)^300` was inferred `Float64`, degrading the `//` operands
#     to Float64 → `MethodError //(::Float64, ::Float64)`.
#   * #9318 — `BigFloat(2)^500` / `BigFloat * BigFloat` were not inferred
#     BigFloat, so `significand`/`exponent` statically bound the generic
#     `where {T<:AbstractFloat}` (Float64 reinterpret) method
#     ("reinterpret(UInt64, BigFloat): size mismatch" / MethodError(::Int64)).
#
# Inference now follows the operand types (the Pow / Mul arms and a `//`
# override), mirroring the runtime PowBigInt / *BigFloat intrinsics and the
# Rational dispatch, so codegen routes through dynamic dispatch / the correct
# static method. Every expected value verified against julia 1.12.6 (default
# 256-bit BigFloat precision).

const BF23 =
    "1.666666666666666666666666666666666666666666666666666666666666666666666666666678"
const RAT_EXACT =
    "2037035976334486086268445688409378161051468393665936250636140449354381299763336706183397376//515377520732011331036461129765621272702107522001"

@testset "inline big(int)//int + BigFloat routes through promote (Issue #9304)" begin
    # Both operand orders; the enclosing `+` must dynamic-dispatch, not take
    # the AddBigFloat fast path with a Rational StructRef operand.
    @test string(BigFloat(1) + big(2) // 3) == BF23
    @test string(big(2) // 3 + BigFloat(1)) == BF23
    @test string(BigFloat(1) + (big(2) // 3)) == BF23

    # The `//` result itself is an exact Rational{BigInt}, not a BigFloat.
    @test typeof(big(2) // 3) === Rational{BigInt}
    @test typeof(2 // big(3)) === Rational{BigInt}
    @test big(2) // 3 == 2 // 3

    # Variable form (already correct before the fix; guarded for parity).
    r = big(2) // 3
    @test string(BigFloat(1) + r) == BF23
end

@testset "big(int)^n // big(int)^m stays exact Rational{BigInt} (Issue #9316)" begin
    @test typeof(big(2)^300) === BigInt
    @test typeof(big(2)^300 // big(3)^100) === Rational{BigInt}
    @test string(big(2)^300 // big(3)^100) == RAT_EXACT

    # Variable form.
    a = big(2)^300
    b = big(3)^100
    @test typeof(a // b) === Rational{BigInt}
    @test string(a // b) == RAT_EXACT
end

@testset "significand/exponent on inline BigFloat ^ and * (Issue #9318)" begin
    @test typeof(BigFloat(2)^500) === BigFloat
    @test significand(BigFloat(2)^500) == big"1.0"
    @test exponent(BigFloat(2)^500) == 500

    @test typeof(BigFloat("2.5") * BigFloat("1.25")) === BigFloat
    @test significand(BigFloat("2.5") * BigFloat("1.25")) == big"1.5625"

    # Variable form (already correct before the fix; guarded for parity).
    x = BigFloat(2)^500
    @test significand(x) == big"1.0"
end

@testset "Pow / Mul inference still correct for non-Big operands (Issue #9318)" begin
    # Regression guards: the Pow/Mul arm changes must not disturb the ordinary
    # numeric result types.
    @test typeof(2^3) === Int64
    @test 2^3 == 8
    @test typeof(2.0^3) === Float64
    @test 2.0^3 == 8.0
    @test typeof(Float32(2)^3) === Float32
    @test typeof(2 // 3) === Rational{Int64}
    @test typeof(big(2)^10) === BigInt
    @test big(2)^10 == 1024
    # Float ^ BigInt keeps the float base type (`Int ^ BigInt` is a separate
    # runtime gap, Issue #9352, so it is not asserted here).
    @test typeof(2.0^big(3)) === Float64
    @test 2.0^big(3) == 8.0
end

true
