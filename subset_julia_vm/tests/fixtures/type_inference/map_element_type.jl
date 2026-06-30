# Test that map infers the result element type from the mapped function (Issue #3480)

using Test

@testset "type_inference_map_element_type: map result element type" begin
    # map(Float64, Int array) -> Vector{Float64}
    result1 = map(Float64, [1, 2, 3])
    @test isa(result1, Array)
    @test result1[1] == 1.0
    @test result1[2] == 2.0

    # map(x -> x * 2, Int array) -> elements are Int
    result2 = map(x -> x * 2, [1, 2, 3])
    @test isa(result2, Array)
    @test result2[1] == 2
    @test result2[2] == 4
    @test result2[3] == 6

    # map returns correct values
    result3 = map(abs, [-1, -2, 3])
    @test result3[1] == 1
    @test result3[2] == 2
    @test result3[3] == 3
end

true
