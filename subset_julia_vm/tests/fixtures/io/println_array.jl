# Test println displays arrays with element type
# Tests that println uses the formatted representation with eltype

using Test

@testset "println displays arrays with element type" begin
    # Test 1: sprint captures println output for vector
    v = [1, 2, 3]
    output_v = sprint(println, v)
    @test occursin("Vector{Int64}", output_v)
    @test occursin("3-element", output_v)

    # Test 2: sprint captures println output for matrix
    m = [1 2; 3 4]
    output_m = sprint(println, m)
    @test occursin("Matrix{Int64}", output_m)
    @test occursin("2×2", output_m)

    # Test 3: Float64 vector
    vf = [1.0, 2.0, 3.0]
    output_vf = sprint(println, vf)
    @test occursin("Vector{Float64}", output_vf)

    # Test 4: Float64 matrix
    mf = [1.0 2.0; 3.0 4.0]
    output_mf = sprint(println, mf)
    @test occursin("Matrix{Float64}", output_mf)
end

true
