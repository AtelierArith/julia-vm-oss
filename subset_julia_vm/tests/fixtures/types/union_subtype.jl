# Test Union subtype checking: Int <: Union{Int, Float64}
# Verifies that subtype operator works with Union types

using Test

@testset "Union type subtype checking: T <: Union{A, B}" begin

    result = 0.0

    # Basic subtype checks: T <: Union{A, B} iff T <: A or T <: B
    # Note: Must assign to variable before using in if condition
    test1 = Int <: Union{Int, Float64}
    if test1 == 1  # Using == 1 since <: returns 0/1
        result = result + 1.0  # Should be true
    end

    test2 = Float64 <: Union{Int, Float64}
    if test2 == 1
        result = result + 1.0  # Should be true
    end

    # String is NOT a subtype of Union{Int, Float64}
    test3 = String <: Union{Int, Float64}
    if test3 == 0
        result = result + 1.0  # Should be false (0)
    end

    # Abstract type in union: Int <: Number, so Int <: Union{Number, String}
    test4 = Int <: Union{Number, String}
    if test4 == 1
        result = result + 1.0  # Int <: Number, so true
    end

    # Union subtype of supertype: Union{A, B} <: T iff A <: T and B <: T
    test5 = Union{Int, Float64} <: Number
    if test5 == 1
        result = result + 1.0  # Both Int <: Number and Float64 <: Number
    end

    # Union{Int, String} is NOT a subtype of Number (String <: Number is false)
    test6 = Union{Int, String} <: Number
    if test6 == 0
        result = result + 1.0
    end

    @test (result) == 6.0
end

true  # Test passed
