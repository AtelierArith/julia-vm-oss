# Test modf and ldexp functions

using Test

@testset "modf (fractional/integer split) and ldexp (x * 2^n)" begin

    # modf: returns (fractional part, integer part)
    r = modf(3.5)
    @assert r[1] == 0.5 "modf fractional part"
    @assert r[2] == 3.0 "modf integer part"

    r2 = modf(-2.75)
    @assert r2[1] == -0.75 "modf negative fractional"
    @assert r2[2] == -2.0 "modf negative integer"

    r3 = modf(5.0)
    @assert r3[1] == 0.0 "modf whole number fractional"
    @assert r3[2] == 5.0 "modf whole number integer"

    # ldexp: x * 2^n
    @assert ldexp(1.0, 0) == 1.0 "ldexp 2^0"
    @assert ldexp(1.0, 1) == 2.0 "ldexp 2^1"
    @assert ldexp(1.0, 3) == 8.0 "ldexp 2^3"
    @assert ldexp(2.5, 2) == 10.0 "ldexp 2.5 * 4"
    @assert ldexp(1.0, -1) == 0.5 "ldexp 2^-1"

    @test (true)
end

true  # Test passed
