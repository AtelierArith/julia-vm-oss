using Test

@testset "Type-level numeric functions" begin
    @test real(Complex{Float32}) == Float32
    @test real(ComplexF64) == Float64
    @test float(Float32) == Float32
    @test float(Int64) == Float64
    @test float(Complex{Int64}) == ComplexF64
    @test float(ComplexF32) == ComplexF32
    @test complex(Int64) == Complex{Int64}
    @test complex(Complex{Float32}) == Complex{Float32}
end

true
