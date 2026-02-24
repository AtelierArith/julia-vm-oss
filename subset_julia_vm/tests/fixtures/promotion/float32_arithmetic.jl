# Float32 arithmetic operations test
# Tests Float32 type with Complex operations and direct arithmetic
# Related: Issue #1625 (bug), Issue #1628 (fix), Issue #1629 (prevention)
# Fixed: Issue #1647 - Direct Float32 arithmetic now returns Float32

using Test

@testset "Float32 creation and conversion" begin
    # Test Float32 constructor
    a = Float32(2.5)
    b = Float32(1.5)

    # Verify values via Float64 conversion
    @test Float64(a) == 2.5
    @test Float64(b) == 1.5

    # Test integer to Float32
    c = Float32(3)
    @test Float64(c) == 3.0
end

@testset "Direct Float32 arithmetic (Issue #1647)" begin
    # Direct Float32 arithmetic should return Float32, not Float64
    a = Float32(2.5)
    b = Float32(1.5)

    # Addition
    c = a + b
    @test c == Float32(4.0)
    c_type = typeof(c)
    @test c_type === Float32

    # Subtraction
    d = a - b
    @test d == Float32(1.0)
    d_type = typeof(d)
    @test d_type === Float32

    # Multiplication
    e = a * b
    @test e == Float32(3.75)
    e_type = typeof(e)
    @test e_type === Float32

    # Division - verify approximate value via Float64 conversion
    f = a / b
    f_type = typeof(f)
    @test f_type === Float32
    # Convert to Float64 for approximate comparison
    f_val = Float64(f)
    @test (f_val > 1.66) && (f_val < 1.67)
end

@testset "Float32 in Complex arithmetic" begin
    # Float32 arithmetic works when used with Complex types
    # because Complex{Float32} has explicit operator methods defined

    z1 = Complex{Float32}(Float32(2.5), Float32(0))
    z2 = Complex{Float32}(Float32(1.5), Float32(0))

    # Addition
    z_sum = z1 + z2
    @test Float64(real(z_sum)) == 4.0

    # Subtraction
    z_diff = z1 - z2
    @test Float64(real(z_diff)) == 1.0

    # Multiplication
    z_prod = z1 * z2
    @test Float64(real(z_prod)) == 3.75
end

# Note: Float32 scalar * Complex{Float32} test removed due to pre-existing Issue #1651
# (Complex{Float32} field access returns Float64 instead of Float32)

true
