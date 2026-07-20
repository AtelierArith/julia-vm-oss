using Test

@testset "typed Complex comprehensions convert through element type" begin
    a = Complex{Float64}[k + 0.5im for k in 1.0:3.0]
    expected = ComplexF64[1.0 + 0.5im, 2.0 + 0.5im, 3.0 + 0.5im]
    @test a == expected
    @test typeof(a) == Vector{ComplexF64}

    alias = ComplexF64[k + 0.5im for k in 1.0:3.0]
    @test alias == expected
    @test typeof(alias) == Vector{ComplexF64}

    constructed = Complex{Float64}[Complex(k, 0.5) for k in 1.0:3.0]
    @test constructed == expected
    @test typeof(constructed) == Vector{ComplexF64}

    real_elements = Complex{Float64}[k for k in 1.0:3.0]
    @test real_elements == ComplexF64[1.0 + 0.0im, 2.0 + 0.0im, 3.0 + 0.0im]
    @test typeof(real_elements) == Vector{ComplexF64}

    filtered = ComplexF64[k + 0.5im for k in 1.0:3.0 if k != 2.0]
    @test filtered == ComplexF64[1.0 + 0.5im, 3.0 + 0.5im]
    @test typeof(filtered) == Vector{ComplexF64}

    f32 = Complex{Float32}[k + 0.5f0im for k in 1.0f0:3.0f0]
    @test f32 == ComplexF32[1.0f0 + 0.5f0im, 2.0f0 + 0.5f0im, 3.0f0 + 0.5f0im]
    @test typeof(f32) == Vector{ComplexF32}

    matrix = ComplexF64[i + j * im for i in 1.0:2.0, j in 0.5:1.5]
    @test matrix == ComplexF64[1.0 + 0.5im 1.0 + 1.5im; 2.0 + 0.5im 2.0 + 1.5im]
    @test typeof(matrix) == Matrix{ComplexF64}
end

true
