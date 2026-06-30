using Test

# Issue #6791: BigFloat division by zero must follow IEEE semantics (±Inf / NaN)
# like Float64, not raise an error. Previously the DivBigFloat intrinsic guarded
# the zero divisor and threw DivisionByZero; astro_float's own div already
# produces the correct ±Inf / NaN.

@testset "BigFloat division by zero (Issue #6791)" begin
    @test big(1.0) / big(0.0) == Inf
    @test big(2.0) / big(0.0) == Inf
    @test big(-1.0) / big(0.0) == -Inf
    @test isnan(big(0.0) / big(0.0))
    @test isinf(big(1.0) / big(0.0))
end

@testset "BigFloat division by zero display (Issue #6791)" begin
    @test string(big(1.0) / big(0.0)) == "Inf"
    @test string(big(-1.0) / big(0.0)) == "-Inf"
    @test string(big(0.0) / big(0.0)) == "NaN"
end

@testset "BigFloat normal division still works (Issue #6791)" begin
    @test big(6.0) / big(2.0) == 3
    @test big(1.0) / big(4.0) == 0.25
end

true
