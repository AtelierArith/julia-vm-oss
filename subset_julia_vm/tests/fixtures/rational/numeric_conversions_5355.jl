# Issue #5355: the numeric type constructors `Float64(r)` / `Float32(r)` /
# `Int(r)` (and the other integer widths) errored on a `Rational` argument
# ("Cannot convert Struct{...} to Float64") because the Rust `convert_to_*`
# helpers only matched primitive values. They now route a Rational through the
# shared `as_rational_parts_i64` helper, matching upstream Julia.
#
# (Earlier a pure-Julia `Float64(x::Rational)` method was tried but was
# nondeterministic on Rational literals due to a const-fold/dispatch-cache
# interaction; the Rust-builtin fix is deterministic.)

using Test

@testset "Rational numeric conversions (Issue #5355)" begin
    # Float conversions = num/den.
    @test Float64(3 // 4) == 0.75
    @test Float64(-7 // 8) == -0.875
    @test Float64(0 // 1) == 0.0
    @test Float32(3 // 4) == 0.75f0
    @test Float64(Float16(3 // 4)) == 0.75
    @test Float64(Rational{Int32}(1, 4)) == 0.25

    # Integer conversions: exact only (den == 1 after normalization).
    @test Int(4 // 2) == 2
    @test Int64(6 // 3) == 2
    @test Int32(8 // 4) == 2
    @test Int8(2 // 1) == 2
    @test UInt8(5 // 1) == 5

    # Non-integer / out-of-range Rationals throw InexactError, like upstream.
    @test_throws InexactError Int(3 // 4)
    @test_throws InexactError UInt8(-1 // 1)
end

true  # Test passed
