# Integer base with a literal negative exponent routes through Base.literal_pow,
# matching upstream Julia (`2^-3` => `0.125`, a Float64). A non-literal negative
# exponent still throws DomainError, also matching upstream (Issue #7233).

using Test

@testset "literal negative integer exponent (Issue #7233)" begin
    # Integer base, literal negative exponent => Float64 via literal_pow
    @test 2^-3 == 0.125
    @test 2^-1 == 0.5
    @test 2^-2 == 0.25
    @test 4^-2 == 0.0625
    @test 8^-1 == 0.125
    @test (2^-3) isa Float64

    # Bound to a name, then used as a literal exponent in an expression
    x = 5
    @test x^-1 == 0.2

    # Positive literal exponents are unchanged (still integer powers)
    @test 2^3 == 8
    @test 2^3 isa Int
    @test 2^0 == 1

    # Float base with literal negative exponent keeps working
    @test 2.0^-3 == 0.125

    # Rational base preserves the Rational result (not widened to Float64)
    @test (1//2)^-2 == 4//1
    @test (1//2)^-2 isa Rational

    # A NON-literal negative exponent still throws DomainError (upstream parity)
    n = -3
    @test_throws DomainError 2^n
    m = -1
    @test_throws DomainError 10^m
end

true
