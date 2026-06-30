using Test

@testset "complex array isa uses logical element type (Issue #3908)" begin
    zeros_complex = zeros(Complex{Float64}, 2)
    erased = Any[zeros_complex]

    @test isa(zeros_complex, Vector{Complex{Float64}})
    @test isa(erased[1], Vector{Complex{Float64}})
    @test !isa(zeros_complex, Vector{Float64})
end

true
