using Test

@testset "sizeof uses logical array element type (Issue #3908)" begin
    complex_values = zeros(Complex{Float64}, 2)

    @test sizeof(complex_values) == 32
    @test typeof(complex_values) == Vector{Complex{Float64}}

    empty_complex = Vector{Complex{Float64}}(undef, 0)

    @test sizeof(empty_complex) == 0
    @test typeof(empty_complex) == Vector{Complex{Float64}}
end

true
