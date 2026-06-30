using Test

@testset "Complex typed array literals preserve eltype (#4018, #4605)" begin
    f64_alias = ComplexF64[1 + 2im, 3 - 4im]
    @test typeof(f64_alias) == Vector{ComplexF64}
    @test eltype(f64_alias) == ComplexF64
    @test typeof(f64_alias[1]) == ComplexF64
    @test f64_alias[1] == 1 + 2im
    @test f64_alias[2] == 3 - 4im

    f64_parametric = Complex{Float64}[1 + 2im, 3 - 4im]
    @test typeof(f64_parametric) == Vector{ComplexF64}
    @test eltype(f64_parametric) == ComplexF64
    @test typeof(f64_parametric[1]) == ComplexF64
    @test f64_parametric[1] == 1 + 2im
    @test f64_parametric[2] == 3 - 4im

    f32_parametric = Complex{Float32}[1 + 2im, 3 + 4im]
    @test typeof(f32_parametric) == Vector{ComplexF32}
    @test eltype(f32_parametric) == ComplexF32
    @test typeof(f32_parametric[1]) == ComplexF32
    @test typeof(real(f32_parametric[1])) == Float32
    @test typeof(imag(f32_parametric[1])) == Float32
    @test real(f32_parametric[1]) == Float32(1)
    @test imag(f32_parametric[1]) == Float32(2)
end

true
