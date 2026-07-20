using Test

@testset "Complex packed storage boundaries and scalar multiplication" begin
    pushed64 = ComplexF64[]
    push!(pushed64, complex(1.0, 2.0))
    @test typeof(pushed64) == Vector{ComplexF64}
    @test typeof(pushed64[1]) == ComplexF64
    @test real(pushed64[1]) == 1.0
    @test imag(pushed64[1]) == 2.0

    pushed32 = ComplexF32[]
    push!(pushed32, complex(1f0, 2f0))
    @test typeof(pushed32) == Vector{ComplexF32}
    @test typeof(pushed32[1]) == ComplexF32

    grown = Vector{ComplexF64}(undef, 0)
    push!(grown, complex(3.0, 4.0))
    @test typeof(grown) == Vector{ComplexF64}
    @test typeof(grown[1]) == ComplexF64

    z1 = Complex(1.0, 2.0)
    z2 = Complex(3.0, 4.0)
    a = [z1, z2]

    result = 2.0 * a
    @test typeof(result) == Vector{ComplexF64}
    @test real(result[1]) == 2.0
    @test imag(result[1]) == 4.0
    @test real(result[2]) == 6.0
    @test imag(result[2]) == 8.0

    result2 = a * 2.0
    @test typeof(result2) == Vector{ComplexF64}
    @test real(result2[1]) == 2.0
    @test imag(result2[1]) == 4.0

    result3 = 3 * a
    @test typeof(result3) == Vector{ComplexF64}
    @test real(result3[1]) == 3.0
    @test imag(result3[1]) == 6.0
end

true
