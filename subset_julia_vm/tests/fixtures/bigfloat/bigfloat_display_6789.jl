using Test

# Issue #6789: BigFloat display must match upstream Julia's Base.MPFR formatting:
# positional decimal for exponents in [-4, 5], otherwise scientific `e±NN`
# (signed, ≥2-digit zero-padded). Previously sjulia emitted astro_float's raw
# scientific form (`5.e+0`, `1.0e+6` without padding, `2.5e-1` for `0.25`).

@testset "BigFloat positional display (Issue #6789)" begin
    @test string(big(5.0)) == "5.0"
    @test string(big(1.5)) == "1.5"
    @test string(big(100.0)) == "100.0"
    @test string(big(1234.5)) == "1234.5"
    @test string(big(0.25)) == "0.25"
    @test string(big(-3.5)) == "-3.5"
    @test string(big(0.0)) == "0.0"
    @test string(big(0.1)) == "0.1000000000000000055511151231257827021181583404541015625"
    @test string(big(0.0001)) == "0.000100000000000000004792173602385929598312941379845142364501953125"
    @test string(big(100000.0)) == "100000.0"
    @test string(big(12345.678)) == "12345.677999999999883584678173065185546875"
end

@testset "BigFloat scientific display (Issue #6789)" begin
    @test string(big(1.0e6)) == "1.0e+06"
    @test string(big(1.0e20)) == "1.0e+20"
    @test string(big(1.0e16)) == "1.0e+16"
    @test string(big(1.5e8)) == "1.5e+08"
    @test string(big(1.0e-5)) == "1.0000000000000000818030539140313095458623138256371021270751953125e-05"
    @test string(big(-1.5e8)) == "-1.5e+08"
end

@testset "BigFloat repr matches string (Issue #6789)" begin
    @test repr(big(5.0)) == "5.0"
    @test repr(big(1.0e20)) == "1.0e+20"
end

true
