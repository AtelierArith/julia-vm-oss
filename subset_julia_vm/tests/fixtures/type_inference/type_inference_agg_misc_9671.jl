# Aggregated concat-safe @testset fixtures (Issue #9671 Phase 3 expansion).
# Each block below is one former standalone fixture, verbatim except its
# `using Test` / trailing `true` were hoisted. @testset names (with their
# original Issue numbers) are preserved, and the #9360 @testset gate still
# detects any per-@testset failure. Source fixture in each banner.
using Test

# ===== source: type_inference/string_mul_non_string.jl =====
# Test that String * non-String raises MethodError like Julia (Issue #3465)


@testset "type_inference_string_mul_non_string: String*Int is MethodError" begin
    # String * String is valid
    @test typeof("a" * "b") == String
    # String * Char is valid
    @test typeof("a" * 'b') == String

    # String * Int should throw MethodError
    @test_throws MethodError "a" * 1
end

# ===== source: type_inference/typeof_datatype_result.jl =====
# Test that typeof transfer function returns DataType, not Top (Issue #3482)


@testset "type_inference_typeof_datatype_result: typeof returns DataType not Top" begin
    @test typeof(typeof(1)) == DataType
    @test typeof(typeof(1.0)) == DataType
    @test typeof(typeof("hello")) == DataType
    @test typeof(typeof(true)) == DataType
end

# ===== source: type_inference/typeof_returns_type.jl =====
# Test that typeof(x) returns a type object, not a String (Issue #3469)


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
