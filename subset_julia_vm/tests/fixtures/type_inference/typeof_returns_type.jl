# Test that typeof(x) returns a type object, not a String (Issue #3469)

using Test

@testset "type_inference_typeof_returns_type: typeof returns DataType, not String" begin
    T = typeof(1)
    @test T === Int64
    @test T == Int64

    T2 = typeof(1.0)
    @test T2 === Float64

    @test typeof("hello") == String
    @test typeof(true) == Bool
end

true
