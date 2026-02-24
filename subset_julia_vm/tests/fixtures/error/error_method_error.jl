using Test

# Tests that calling functions with no matching method raises a catchable error
# (Issue #2918)

# Define a function with a specific type constraint
function typed_add(x::Int64, y::Int64)
    return x + y
end

function method_error_wrong_types_caught()
    caught = false
    try
        # Calling with String args when Int64 is required → MethodError
        result = typed_add("hello", "world")
    catch e
        caught = true
    end
    return caught
end

function method_error_no_error_on_correct_types()
    caught = false
    try
        result = typed_add(1, 2)
    catch e
        caught = true
    end
    return caught
end

@testset "method error raises catchable exception (Issue #2918)" begin
    @test method_error_wrong_types_caught()
    @test !method_error_no_error_on_correct_types()
end

true
