# Issue #5356: the transcendental math functions (exp/sin/cos/tan/log/...) are
# pure-Julia and only had `::Float64`/`::Int64` methods. A `Rational` argument
# was lenient-dispatched to the `::Float64` method, landing the Rational in a
# Float64 slot -> "InternalError: LoadSlotF64: expected F64-compatible value".
# (`sqrt` worked because it is a Rust builtin using `pop_f64_or_i64`.)
#
# Fix: add `f(x::Rational) = f(float(x))` for the base functions (exp, sin, cos,
# tan, asin, acos, atan in special/, log in special/, cbrt in math.jl). The
# derived functions (sinh/cosh/tanh/log2/log10/exp2/exp10/expm1/log1p) cascade
# through these. `float(::Rational)` is Float64, so each reduces to the Float64
# method with no recursion.

using Test

@testset "math functions accept Rational args (Issue #5356)" begin
    # Each Rational result equals the corresponding `float(x)` computation.
    @test exp(1 // 6) == exp(1 / 6)
    @test sin(1 // 6) == sin(1 / 6)
    @test cos(1 // 3) == cos(1 / 3)
    @test tan(1 // 4) == tan(1 / 4)
    @test log(3 // 2) == log(3 / 2)
    @test asin(1 // 2) == asin(1 / 2)
    @test acos(1 // 2) == acos(1 / 2)
    @test atan(1 // 1) == atan(1 / 1)
    @test cbrt(8 // 1) == cbrt(8 / 1)
    @test exp(0 // 1) == 1.0
    @test log(1 // 1) == 0.0

    # Derived functions cascade through the base ones.
    @test sinh(1 // 2) == sinh(1 / 2)
    @test cosh(1 // 2) == cosh(1 / 2)
    @test log2(8 // 1) == log2(8 / 1)
    @test exp2(2 // 1) == exp2(2 / 1)

    # Via a variable slot (the original LoadSlotF64 trigger).
    r = 1 // 6
    @test exp(r) == exp(1 / 6)
    @test sin(r) == sin(1 / 6)
end

true  # Test passed
