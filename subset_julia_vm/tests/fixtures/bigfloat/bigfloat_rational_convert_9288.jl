using Test

# Issue #9288: mixed BigFloat–Rational arithmetic errored with
# "Cannot convert StructRef(..) to BigFloat".
#
# The generic Number promote-fallback for `+ - * / ==` widens the pair via
# `promote(::BigFloat, ::Rational)`, whose convert leg
# `convert(::Type{BigFloat}, ::Rational)` routes (through the number-convert
# fallback `convert(::Type{T}, x::Number) = T(x)`) to `BigFloat(::Rational)`.
# `BigFloat(x)` is compiled to a direct `CallBuiltin(BigFloat, 1)`, so a
# pure-Julia `BigFloat(::Rational)` method could never be dispatched; the
# Rust builtin only handled Irrational structs and threw on a Rational
# StructRef, so `promote` could not terminate and every mixed op failed.
#
# The BigFloat builtin now converts a Rational{T} as
# `BigFloat(numerator) / BigFloat(denominator)` at the active precision,
# mirroring upstream base/mpfr.jl `BigFloat(x::Rational)`. All expected
# strings verified against julia 1.12.6 (default 256-bit BigFloat precision).

@testset "BigFloat(::Rational) conversion (Issue #9288)" begin
    @test typeof(BigFloat(1 // 3)) === BigFloat
    @test string(BigFloat(1 // 3)) ==
          "0.3333333333333333333333333333333333333333333333333333333333333333333333333333348"
    @test typeof(convert(BigFloat, 1 // 3)) === BigFloat
    @test string(convert(BigFloat, 1 // 3)) ==
          "0.3333333333333333333333333333333333333333333333333333333333333333333333333333348"
    # Negative and non-unit numerator/denominator.
    @test string(BigFloat(-2 // 7)) ==
          "-0.2857142857142857142857142857142857142857142857142857142857142857142857142857137"
end

@testset "mixed BigFloat–Rational division (Issue #9288)" begin
    @test typeof(BigFloat(1) / (1 // 3)) === BigFloat
    @test string(BigFloat(1) / (1 // 3)) == "3.0"
    @test string((1 // 3) / BigFloat(1)) ==
          "0.3333333333333333333333333333333333333333333333333333333333333333333333333333348"
    @test string(BigFloat(2) / (3 // 4)) ==
          "2.666666666666666666666666666666666666666666666666666666666666666666666666666678"
end

@testset "mixed BigFloat–Rational addition/subtraction (Issue #9288)" begin
    @test string(BigFloat(1) + (1 // 3)) ==
          "1.333333333333333333333333333333333333333333333333333333333333333333333333333339"
    @test string((1 // 3) + BigFloat(1)) ==
          "1.333333333333333333333333333333333333333333333333333333333333333333333333333339"
    @test string(BigFloat(1) - (1 // 3)) ==
          "0.6666666666666666666666666666666666666666666666666666666666666666666666666666609"
    @test string((1 // 3) - BigFloat(1)) ==
          "-0.6666666666666666666666666666666666666666666666666666666666666666666666666666609"
end

@testset "mixed BigFloat–Rational multiplication (Issue #9288)" begin
    @test string(BigFloat(1) * (1 // 3)) ==
          "0.3333333333333333333333333333333333333333333333333333333333333333333333333333348"
    @test string((1 // 3) * BigFloat(1)) ==
          "0.3333333333333333333333333333333333333333333333333333333333333333333333333333348"
end

@testset "mixed BigFloat–Rational comparison (Issue #9288)" begin
    @test BigFloat(1) == (1 // 1)
    @test (1 // 1) == BigFloat(1)
    @test !(BigFloat(1) == (2 // 1))
    @test !(BigFloat(1) < (1 // 3))
    @test (1 // 3) < BigFloat(1)
end

@testset "mixed BigFloat–Rational result type (Issue #9288)" begin
    # The promote-fallback must widen to BigFloat, not degrade to Float64.
    @test typeof(BigFloat(1) + 1 // 3) === BigFloat
    @test typeof((1 // 3) * BigFloat(1)) === BigFloat
end

# Regression for the fix-forward on Issue #9288: `BigFloat(x)` lowers to a direct
# `CallBuiltin(BigFloat, 1)`, and the Rust builtin resolves a `Value::StructRef`
# Rational through the shared `struct_heap`. That resolution is frame-independent
# — there is a single VM struct_heap, not a per-call-frame one — so the Rational
# conversion must behave identically whether it reaches the builtin at top level
# or from a function / lambda / nested-closure / setprecision-closure frame. The
# top-level testsets above cannot exercise the StructRef-from-a-frame path, so the
# rows below pin it. All expected strings verified against julia 1.12.6.
@testset "BigFloat(::Rational) from a function frame (Issue #9288)" begin
    g(x) = BigFloat(1) + x
    @test typeof(g(1 // 3)) === BigFloat
    @test string(g(1 // 3)) ==
          "1.333333333333333333333333333333333333333333333333333333333333333333333333333339"

    function h()
        BigFloat(1) / (1 // 3)
    end
    @test string(h()) == "3.0"

    # == mixed comparison, both orders, inside a function frame.
    function eqf()
        (BigFloat(1) == 2 // 2, 1 // 2 == BigFloat(0.5))
    end
    @test eqf() == (true, true)
end

@testset "BigFloat(::Rational) from a lambda / closure frame (Issue #9288)" begin
    f = () -> BigFloat(1 // 3)
    @test typeof(f()) === BigFloat
    @test string(f()) ==
          "0.3333333333333333333333333333333333333333333333333333333333333333333333333333348"

    # Nested inner function.
    function outer()
        inner(y) = BigFloat(1) + y
        inner(1 // 7)
    end
    @test string(outer()) ==
          "1.142857142857142857142857142857142857142857142857142857142857142857142857142855"

    # Closure capturing a Rational local, converted from the closure body.
    function mk()
        r = 1 // 9
        () -> BigFloat(r)
    end
    @test string(mk()()) ==
          "0.1111111111111111111111111111111111111111111111111111111111111111111111111111109"

    # Rational{BigInt} through a function frame.
    bigrat() = BigFloat(big(1) // big(3))
    @test string(bigrat()) ==
          "0.3333333333333333333333333333333333333333333333333333333333333333333333333333348"
end

@testset "BigFloat(::Rational) from a setprecision closure (Issue #9288)" begin
    @test string(setprecision(() -> BigFloat(1 // 3), BigFloat, 64)) ==
          "0.333333333333333333342"
    r = setprecision(BigFloat, 128) do
        BigFloat(1 // 3)
    end
    @test string(r) == "0.3333333333333333333333333333333333333338"
end

true
