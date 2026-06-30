# `Float64 ^ Integer` must use upstream Julia's compensated power-by-squaring
# (`pow_body(x::Float64, n::Integer)` in base/special/pow.jl), not a plain
# `powf`/`powi`. Those differ by ~1 ULP in many inexact cases, e.g. `10.0^-2`
# is `0.010000000000000002` upstream but was `0.01` here (Issue #7308, follow-up
# to #7233). Every expected value below was taken verbatim from Julia 1.12.6.

using Test

@testset "Float64^Int compensated power (Issue #7308)" begin
    # The headline regressions: integer base via literal_pow (#7233) widens to
    # Float64 then raises, and the Float64 path itself must match upstream.
    @test 10^-2 === 0.010000000000000002
    @test 10.0^-2 === 0.010000000000000002

    # Other inexact cases that diverged by 1 ULP under the old powf/powi path.
    @test 2.3^3 === 12.166999999999996
    @test 2.6^3 === 17.576000000000004
    @test 3.2^-2 === 0.09765625
    @test 3.8^-2 === 0.06925207756232686
    @test 4.1^3 === 68.92099999999999
    @test 5.0^-2 === 0.04000000000000001
    @test 5.9^3 === 205.37900000000002
    @test 6.5^-2 === 0.02366863905325444
    @test 7.4^-2 === 0.01826150474799123
    @test 8.6^-2 === 0.01352082206598161
    @test 9.2^3 === 778.6879999999998
    @test 9.5^-2 === 0.011080332409972297
    @test 10.7^-2 === 0.008734387282732119

    # Larger / longer exponent chains still match upstream exactly.
    @test 7.0^-5 === 5.9499018266198606e-5
    @test 3.0^-3 === 0.037037037037037035
    @test 1.5^-1 === 0.6666666666666666
    @test 1.5^-2 === 0.4444444444444444
    @test 1.5^-3 === 0.2962962962962963

    # Exact-binary cases must stay exact (no regression from the new reduction).
    @test 2.0^3 === 8.0
    @test 2.0^-3 === 0.125
    @test 2.0^-2 === 0.25
    @test 2.0^-1 === 0.5
    @test 4.0^-2 === 0.0625
    @test 2^-3 === 0.125
    @test 4^-2 === 0.0625
    @test 1.5^10 === 57.6650390625
    @test 7.0^5 === 16807.0
    @test 13.0^4 === 28561.0

    # Non-negative exponents are unaffected.
    @test 10.0^2 === 100.0
    @test 3.0^3 === 27.0
    @test 10.0^10 === 1.0e10
    @test 10.0^0 === 1.0
    @test 10.0^1 === 10.0

    # The result type stays Float64 throughout.
    @test (10.0^-2) isa Float64
    @test (10^-2) isa Float64
    @test (2.0^3) isa Float64
end

true
