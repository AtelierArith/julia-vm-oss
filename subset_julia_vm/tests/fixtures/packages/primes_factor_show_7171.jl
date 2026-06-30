# Issue #7171: Primes.Factorization prints in the upstream `p1^e1 ⋅ p2^e2` form
# rather than dumping the raw struct fields. Exercises that a `Base.show` defined
# inside a package module is dispatched by `print`/`println`/`string` even though
# the value's type name is module-qualified ("Primes.Factorization").
using Primes
using Test

@testset "Issue #7171: Factorization show" begin
    @test string(factor(360)) == "2^3 ⋅ 3^2 ⋅ 5"
    @test string(factor(97)) == "97"        # single prime, exponent 1 omitted
    @test string(factor(2)) == "2"
    @test string(factor(1)) == "1"          # empty factorization
    @test string(factor(1000000)) == "2^6 ⋅ 5^6"
end

true
