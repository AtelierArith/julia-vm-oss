using Test

@testset "Meta.parse(repr(typemin(Int64))) round-trips (Issue #4753)" begin
    s = repr(typemin(Int64))
    @test s == "-9223372036854775808"
    parsed = Meta.parse(s)
    @test eval(parsed) == typemin(Int64)
    @test typeof(eval(parsed)) === Int64
end

@testset "Meta.parse promotes overflowing integer literals (Issue #4753)" begin
    # Magnitude of typemin(Int64). On its own, it overflows i64 but
    # fits i128 — sjulia should promote rather than error.
    @test eval(Meta.parse("9223372036854775808")) == 9223372036854775808
    # Very large literal: should promote further to BigInt.
    big = eval(Meta.parse("99999999999999999999999999999999"))
    @test big > 0
    @test string(big) == "99999999999999999999999999999999"
end

@testset "Meta.parse non-typemin negatives unaffected (Issue #4753)" begin
    @test eval(Meta.parse("-1")) === -1
    @test eval(Meta.parse("-100")) === -100
    @test eval(Meta.parse("-9223372036854775807")) === -9223372036854775807
end

true
