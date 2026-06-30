using Test

@testset "IndexStore uses logical array element type (Issue #3908)" begin
    complex_values = zeros(Complex{Float64}, 2)
    complex_values[1] = 2.5
    complex_values[2] = -3

    @test complex_values[1] == 2.5 + 0.0im
    @test complex_values[2] == -3.0 + 0.0im
    @test typeof(complex_values) == Vector{Complex{Float64}}
    @test typeof(complex_values[1]) == Complex{Float64}

    bool_values = Vector{Bool}(undef, 2)
    bool_values[1] = 1
    bool_values[2] = 0

    @test bool_values[1] == true
    @test bool_values[2] == false
    @test eltype(bool_values) == Bool
end

true
