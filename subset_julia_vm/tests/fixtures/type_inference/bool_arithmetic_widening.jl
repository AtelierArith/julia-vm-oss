# Test Bool arithmetic widens to Int64, not Bool (Issue #3462)

using Test

@testset "type_inference_bool_arithmetic_widening: Bool+Bool yields Int64" begin
    @test typeof(true + false) == Int64
    @test typeof(true + true) == Int64
    @test typeof(false + 1) == Int64
end

true
