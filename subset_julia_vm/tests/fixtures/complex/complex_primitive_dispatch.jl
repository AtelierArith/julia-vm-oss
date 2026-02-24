using Test

# Test Complex + non-Int64/Float64 primitive dispatch (Issue #2235).
# Before the fix, dispatch checks in binary.rs only covered Int64/Float64,
# so Complex + Float32 or Complex + Bool would miss specialized paths.

@testset "Complex + Float32" begin
    z = Complex{Float64}(1.0, 2.0)
    x = Float32(3.0)
    result = z + x
    @test real(result) == 4.0
    @test imag(result) == 2.0
end

@testset "Complex + Bool" begin
    z = Complex{Float64}(1.0, 2.0)
    result = z + true
    @test real(result) == 2.0
    @test imag(result) == 2.0
end

@testset "Float32 + Complex" begin
    x = Float32(5.0)
    z = Complex{Float64}(1.0, 2.0)
    result = x + z
    @test real(result) == 6.0
    @test imag(result) == 2.0
end

@testset "Complex * Float32" begin
    z = Complex{Float64}(2.0, 3.0)
    x = Float32(2.0)
    result = z * x
    @test real(result) == 4.0
    @test imag(result) == 6.0
end

true
