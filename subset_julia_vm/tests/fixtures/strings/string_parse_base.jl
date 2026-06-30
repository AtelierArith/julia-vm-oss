# string(x; base=N) and parse(T, s; base=N) - number base conversion (Issue #2036)

using Test

@testset "string(x; base=N)" begin
    # Hexadecimal (base 16)
    @test string(255, base=16) == "ff"
    @test string(0, base=16) == "0"
    @test string(16, base=16) == "10"

    # Binary (base 2)
    @test string(255, base=2) == "11111111"
    @test string(0, base=2) == "0"
    @test string(10, base=2) == "1010"

    # Octal (base 8)
    @test string(255, base=8) == "377"
    @test string(8, base=8) == "10"
    @test string(10, base=8) == "12"

    # Decimal (base 10) - identity
    @test string(42, base=10) == "42"
end

@testset "parse(Int, s; base=N)" begin
    # Hexadecimal (base 16)
    @test parse(Int, "ff", base=16) == 255
    @test parse(Int, "10", base=16) == 16
    @test parse(Int, "0", base=16) == 0

    # Binary (base 2)
    @test parse(Int, "11111111", base=2) == 255
    @test parse(Int, "1010", base=2) == 10

    # Octal (base 8)
    @test parse(Int, "377", base=8) == 255
    @test parse(Int, "12", base=8) == 10

    # Decimal (base 10)
    @test parse(Int, "42", base=10) == 42
end

@testset "round-trip: parse(Int, string(x; base=N); base=N) == x" begin
    @test parse(Int, string(100, base=16), base=16) == 100
    @test parse(Int, string(42, base=2), base=2) == 42
    @test parse(Int, string(73, base=8), base=8) == 73
end

# Issue #7875 (docs/COMPARISION.md P1): parse(Int, s; base=N) is now Pure Julia
# (`_parse_int_base` wrapping `_tryparse_int` in base/parse.jl); the compiler
# rewrites the kwargs form to a positional call instead of the former
# `StringToIntBase` Rust builtin. These cover the edge cases the migration must
# preserve (sign, surrounding whitespace, max base-36 digit, mixed-case hex).
@testset "parse(Int, s; base=N) pure-Julia migration (#7875)" begin
    @test parse(Int, "  -101  ", base=2) == -5
    @test parse(Int, "+ff", base=16) == 255
    @test parse(Int, "z", base=36) == 35
    @test parse(Int, "DEAD", base=16) == 57005
    @test parse(Int, "dead", base=16) == 57005
end

# Issue #7942: `_` is a digit separator only in numeric *literals* in source
# code, not in parse()/tryparse() string input. Upstream julia throws
# ArgumentError (parse) / returns nothing (tryparse). The pre-existing
# underscore-skip in `_tryparse_int` (introduced by #2566) was removed, fixing
# the base-10 path and keeping the migrated base-N path upstream-faithful.
@testset "parse/tryparse reject underscores (#7942)" begin
    @test tryparse(Int, "1_000") === nothing
    @test_throws ArgumentError parse(Int, "1_000")
    @test_throws ArgumentError parse(Int, "ff_ff", base=16)
    # plain digit strings still parse correctly
    @test parse(Int, "1000") == 1000
    @test parse(Int, "ffff", base=16) == 65535
end

true
