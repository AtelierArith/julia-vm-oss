# Test that typeof transfer function returns DataType, not Top (Issue #3482)

using Test

@testset "type_inference_typeof_datatype_result: typeof returns DataType not Top" begin
    @test typeof(typeof(1)) == DataType
    @test typeof(typeof(1.0)) == DataType
    @test typeof(typeof("hello")) == DataType
    @test typeof(typeof(true)) == DataType
end

true
