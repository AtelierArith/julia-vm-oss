# Test Complex predicates (Issue #475)
# Based on Julia's base/complex.jl:150-152

using Test

@testset "Complex predicates: isfinite, isnan, isinf (Issue #475)" begin

    result = 0.0

    # Test isfinite for finite complex
    if isfinite(Complex(1.0, 2.0))
        result = result + 1.0
    end

    # Test isfinite with Inf real part
    if !isfinite(Complex(Inf, 0.0))
        result = result + 1.0
    end

    # Test isfinite with Inf imaginary part
    if !isfinite(Complex(0.0, Inf))
        result = result + 1.0
    end

    # Test isfinite with NaN
    if !isfinite(Complex(NaN, 0.0))
        result = result + 1.0
    end

    # Test isnan for finite complex
    if !isnan(Complex(1.0, 2.0))
        result = result + 1.0
    end

    # Test isnan with NaN real part
    if isnan(Complex(NaN, 0.0))
        result = result + 1.0
    end

    # Test isnan with NaN imaginary part
    if isnan(Complex(0.0, NaN))
        result = result + 1.0
    end

    # Test isinf for finite complex
    if !isinf(Complex(1.0, 2.0))
        result = result + 1.0
    end

    # Test isinf with Inf real part
    if isinf(Complex(Inf, 0.0))
        result = result + 1.0
    end

    # Test isinf with -Inf imaginary part
    if isinf(Complex(0.0, -Inf))
        result = result + 1.0
    end

    @test (result) == 10.0
end

true  # Test passed
