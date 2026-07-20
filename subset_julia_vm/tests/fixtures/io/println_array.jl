# Test println displays arrays

using Test

@testset "println displays arrays" begin
    # Test 1: sprint captures println output for vector
    v = [1, 2, 3]
    output_v = sprint(println, v)
    @test output_v == "[1, 2, 3]\n"

    # Test 2: sprint captures println output for matrix
    m = [1 2; 3 4]
    output_m = sprint(println, m)
    @test output_m == "[1 2; 3 4]\n"

    # Test 3: Float64 vector
    vf = [1.0, 2.0, 3.0]
    output_vf = sprint(println, vf)
    @test output_vf == "[1.0, 2.0, 3.0]\n"

    # Test 4: Float64 matrix
    mf = [1.0 2.0; 3.0 4.0]
    output_mf = sprint(println, mf)
    @test output_mf == "[1.0 2.0; 3.0 4.0]\n"
end

true
