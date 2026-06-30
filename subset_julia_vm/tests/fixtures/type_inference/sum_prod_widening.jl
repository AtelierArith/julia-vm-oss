# Test that sum/prod apply Julia's reduction widening rules (Issue #3478)

using Test

@testset "type_inference_sum_prod_widening: sum/prod use widening rules" begin
    # Bool array -> Int64
    bools = [true, false, true]
    @test typeof(sum(bools)) == Int64
    # Int64 array -> Int64
    ints = [1, 2, 3]
    @test typeof(sum(ints)) == Int64
    @test typeof(prod(ints)) == Int64
    # Float64 array -> Float64
    floats = [1.0, 2.0, 3.0]
    @test typeof(sum(floats)) == Float64
end

true
