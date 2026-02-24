using Test

# Test generic Complex arithmetic dispatch via Julia's type hierarchy
# Complex{T} <: Number enables promotion-based dispatch for any type combination

@testset "Generic Complex + Real dispatch" begin
    # Complex{Float64} + Float32 (no concrete method, uses generic +(::Complex, ::Real))
    z = Complex{Float64}(1.0, 2.0)
    x = Float32(3.0)
    result = z + x
    @test real(result) == 4.0
    @test imag(result) == 2.0

    # Float32 + Complex{Float64}
    result2 = x + z
    @test real(result2) == 4.0
    @test imag(result2) == 2.0
end

@testset "Generic Complex + Bool dispatch" begin
    z = Complex{Float64}(1.0, 2.0)
    result = z + true
    @test real(result) == 2.0
    @test imag(result) == 2.0
end

@testset "Generic Complex * Real dispatch" begin
    z = Complex{Float64}(2.0, 3.0)
    x = Float32(2.0)
    result = z * x
    @test real(result) == 4.0
    @test imag(result) == 6.0
end

@testset "Generic Complex - Real dispatch" begin
    z = Complex{Float64}(5.0, 3.0)
    result = z - Float32(2.0)
    @test real(result) == 3.0
    @test imag(result) == 3.0

    result2 = Float32(10.0) - z
    @test real(result2) == 5.0
    @test imag(result2) == -3.0
end

@testset "Generic mixed Complex+Complex dispatch" begin
    # Complex{Float32} + Complex{Int64} (no concrete method existed before)
    z1 = Complex{Float32}(Float32(1.0), Float32(2.0))
    z2 = Complex{Int64}(3, 4)
    result = z1 + z2
    @test real(result) == 4.0
    @test imag(result) == 6.0
end

@testset "Generic Complex constructor with mixed types" begin
    # Complex(Int64, Float64) -> Complex{Float64}
    z = Complex(1, 2.0)
    @test real(z) == 1.0
    @test imag(z) == 2.0

    # Complex(Float32, Int64) -> Complex{Float32}
    z2 = Complex(Float32(1.0), 2)
    @test real(z2) == Float32(1.0)
    @test imag(z2) == Float32(2.0)
end

true
