# Test that method errors are raised properly for missing method matches
# This tests the fix for issue #1599 - avoiding panic!() in VM error paths

using Test

# Define a custom type for testing
struct CustomType
    value::Int64
end

@testset "Method error handling" begin
    # Test that calling an undefined method on a custom type throws MethodError
    caught_method_error = false
    try
        x = CustomType(42)
        # Try to negate a custom type with no neg method defined
        -x
    catch e
        caught_method_error = true
        @test isa(e, MethodError)
    end
    @test caught_method_error

    # Test that binary operations on incompatible types throw MethodError
    caught_binary_error = false
    try
        x = CustomType(1)
        y = CustomType(2)
        # No + method defined for CustomType
        x + y
    catch e
        caught_binary_error = true
        @test isa(e, MethodError)
    end
    @test caught_binary_error
end

true
