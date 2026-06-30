using Test

@testset "in uses logical array elements (Issue #3908)" begin
    complex_values = zeros(Complex{Float64}, 2)
    complex_values[1] = 1 + 0im
    complex_values[2] = 1 + 2im

    @test (1 + 0im) in complex_values
    @test 1 in complex_values
    @test !((2 + 0im) in complex_values)

    non_real_values = zeros(Complex{Float64}, 1)
    non_real_values[1] = 1 + 2im

    @test !(1 in non_real_values)
end

true
