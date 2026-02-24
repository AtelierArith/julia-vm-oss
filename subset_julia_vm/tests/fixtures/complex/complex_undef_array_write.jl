# Complex undef array write-read
# Tests writing to and reading from Vector{Complex{Float64}}(undef, n).
# Related: Issue #1801

using Test

@testset "Complex undef array write and read" begin
    v = Vector{Complex{Float64}}(undef, 3)
    @test length(v) == 3

    # Write complex values
    v[1] = Complex(1.0, 2.0)
    v[2] = Complex(3.0, 4.0)
    v[3] = Complex(5.0, 6.0)

    # Read back and verify
    @test real(v[1]) == 1.0
    @test imag(v[1]) == 2.0
    @test real(v[2]) == 3.0
    @test imag(v[2]) == 4.0
    @test real(v[3]) == 5.0
    @test imag(v[3]) == 6.0
end

@testset "Complex undef array overwrite" begin
    v = Vector{Complex{Float64}}(undef, 2)

    # Write initial values
    v[1] = Complex(10.0, 20.0)
    v[2] = Complex(30.0, 40.0)

    # Overwrite
    v[1] = Complex(100.0, 200.0)

    @test real(v[1]) == 100.0
    @test imag(v[1]) == 200.0
    @test real(v[2]) == 30.0
    @test imag(v[2]) == 40.0
end

true
