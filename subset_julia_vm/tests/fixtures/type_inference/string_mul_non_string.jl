# Test that String * non-String raises MethodError like Julia (Issue #3465)

using Test

@testset "type_inference_string_mul_non_string: String*Int is MethodError" begin
    # String * String is valid
    @test typeof("a" * "b") == String
    # String * Char is valid
    @test typeof("a" * 'b') == String

    # String * Int should throw MethodError
    @test_throws MethodError "a" * 1
end

true
