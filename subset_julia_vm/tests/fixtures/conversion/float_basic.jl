# Test float() function - basic type conversions
# float(x::Int64) -> Float64
# float(x::Float64) -> Float64 (identity)
# float(x::Rational) -> Float64
# float(x::Complex) -> Complex{Float64}

using Test

@testset "float() function - Int64, Float64, Rational, Complex conversions" begin

    result = 0.0

    # Test Int64 -> Float64
    result = result + float(42)
    if result != 42.0
        error("float(42) should be 42.0")
    end

    # Test Float64 -> Float64 (identity)
    result = result + float(3.14)
    if result != 45.14
        error("float(3.14) should be 3.14")
    end

    # Test Rational -> Float64
    result = result + float(3//2)
    if result != 46.64
        error("float(3//2) should be 1.5")
    end

    # Test Complex -> Complex{Float64}
    z = Complex(1, 2)
    zf = float(z)
    if typeof(zf) != Complex{Float64}
        error("float(Complex(1, 2)) should return Complex{Float64}")
    end
    # Use struct field access instead of real()/imag() due to method dispatch limitation
    # with runtime-created structs from float() builtin
    if zf.re != 1.0 || zf.im != 2.0
        error("float(Complex(1, 2)) should be Complex(1.0, 2.0)")
    end

    @test isapprox((result), 46.64)
end

true  # Test passed
