using Test

@testset "empty Complex typed array push resolves packed values (Issues #9407/#9492/#9737)" begin
    z64 = Complex{Float64}(1.0, 2.0)
    a64 = Complex{Float64}[]
    push!(a64, z64)
    @test length(a64) == 1
    @test a64[1] === z64
    @test eltype(a64) === ComplexF64
    @test typeof(a64) === Vector{ComplexF64}

    alias64 = ComplexF64[]
    push!(alias64, z64)
    @test alias64[1] === z64
    @test typeof(alias64) === Vector{ComplexF64}

    undef64 = Vector{ComplexF64}(undef, 0)
    push!(undef64, z64)
    @test undef64[1] === z64
    @test typeof(undef64) === Vector{ComplexF64}

    z32 = ComplexF32(1.0f0, 2.0f0)
    a32 = ComplexF32[]
    push!(a32, z32)
    @test a32[1] === z32
    @test typeof(a32) === Vector{ComplexF32}

    real_to_complex = ComplexF64[]
    push!(real_to_complex, 1.5)
    @test real_to_complex[1] === ComplexF64(1.5, 0.0)
    @test typeof(real_to_complex[1]) === ComplexF64
end

@testset "Complex scalar read from packed storage multiplies Complex vectors" begin
    scalar = (ComplexF64[2.0 + 3.0im])[1]
    vector = ComplexF64[1.0 + 0.0im, 0.0 + 1.0im]
    @test scalar * vector == ComplexF64[2.0 + 3.0im, -3.0 + 2.0im]
end

true
