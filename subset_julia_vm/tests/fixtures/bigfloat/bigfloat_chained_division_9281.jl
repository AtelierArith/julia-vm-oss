using Test

# Issue #9281: a chained division whose left operand is a boxed non-Float64
# numeric intermediate (e.g. `BigFloat / BigFloat / Int`) must keep the
# intermediate's type instead of narrowing it to Float64. The compiler's
# `infer_expr_type` / `infer_julia_type` unconditionally reported `Float64`
# for every `BinaryOp::Div`, so the *inner* `BigFloat / BigFloat` (correctly
# a BigFloat at runtime) was inferred as Float64; the outer `(…) / 2` then
# picked the Float64 fast path (`DynamicToF64; ToF64; DivF64`) and degraded
# the result. The same root cause degraded chained Float32/Float16/BigInt and
# Rational divisions. Division inference now follows the operand types
# (integer/integer → Float64, but BigFloat/BigInt/narrow-float/struct operands
# are preserved), mirroring the runtime `DivBigFloat`/`DivF*` dispatch and the
# lattice `tfunc_div`. All expected strings verified against julia 1.12.6
# (default 256-bit BigFloat precision).

@testset "chained BigFloat division keeps BigFloat (Issue #9281)" begin
    # The exact MWE from the issue: (BigFloat / BigFloat) / Int stays BigFloat.
    x = BigFloat("1.0") / BigFloat("3.0") / 2
    @test typeof(x) === BigFloat
    @test string(x) ==
          "0.1666666666666666666666666666666666666666666666666666666666666666666666666666674"

    # A longer chain keeps widening the Int operands to BigFloat at each step.
    y = BigFloat("1.0") / BigFloat("3.0") / 2 / 5
    @test typeof(y) === BigFloat
    @test string(y) ==
          "0.03333333333333333333333333333333333333333333333333333333333333333333333333333359"

    # Single mixed ops (already correct, guarded here): BigFloat / Int and
    # Int / BigFloat both widen the integer to BigFloat.
    @test typeof(BigFloat("1.0") / 3) === BigFloat
    @test string(BigFloat("1.0") / 3) ==
          "0.3333333333333333333333333333333333333333333333333333333333333333333333333333348"
    @test typeof(2 / BigFloat("3.0")) === BigFloat
    @test string(2 / BigFloat("3.0")) ==
          "0.6666666666666666666666666666666666666666666666666666666666666666666666666666695"

    # A single BigFloat / BigFloat also stays BigFloat.
    @test typeof(BigFloat("1.0") / BigFloat("3.0")) === BigFloat
end

@testset "chained division preserves sibling numeric types (Issue #9281)" begin
    # The same fix keeps narrow-float, BigInt, and Rational chains from
    # degrading to Float64.
    f32 = Float32(1) / Float32(3) / 2
    @test typeof(f32) === Float32
    @test string(f32) == "0.16666667"

    f16 = Float16(1) / Float16(3) / 2
    @test typeof(f16) === Float16
    @test string(f16) == "0.1666"

    # BigInt `/` yields BigFloat (float division), preserved through the chain.
    bi = big(1) / big(3) / 2
    @test typeof(bi) === BigFloat
    @test string(bi) ==
          "0.1666666666666666666666666666666666666666666666666666666666666666666666666666674"

    rat = (1 // 2) / 3 / 2
    @test typeof(rat) === Rational{Int64}
    @test rat == 1 // 12
end

@testset "chained division still floats pure-integer results (Issue #9281)" begin
    # Regression guard: integer/integer division still yields Float64, and an
    # all-Float64 chain is unchanged.
    @test typeof(1 / 3 / 2) === Float64
    @test 1 / 3 / 2 == 0.16666666666666666
    @test typeof(1.0 / 3.0 / 2) === Float64
end

@testset "chained division inside functions (Issue #9281)" begin
    # Typed parameters: the concrete BigFloat type flows through the body.
    g(a::BigFloat, b::BigFloat) = a / b / 2
    @test typeof(g(BigFloat("1.0"), BigFloat("3.0"))) === BigFloat

    # Untyped parameters resolve dynamically at runtime and must not force a
    # Float64 slot for the boxed BigFloat intermediate.
    h(a, b) = a / b / 2
    @test typeof(h(BigFloat("1.0"), BigFloat("3.0"))) === BigFloat
    @test typeof(h(Float32(1), Float32(3))) === Float32
    @test typeof(h(1, 3)) === Float64
end

true
