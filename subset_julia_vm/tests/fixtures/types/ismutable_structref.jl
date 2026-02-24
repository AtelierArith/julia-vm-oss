# Test ismutable with StructRef (mutable structs on heap)
# Bug fix: ismutable was not handling StructRef, only Value::Struct

using Test

mutable struct MutablePoint
    x::Int64
    y::Int64
end

struct ImmutablePoint
    x::Int64
    y::Int64
end

@testset "ismutable with StructRef (mutable structs on heap)" begin



    m = MutablePoint(1, 2)
    i = ImmutablePoint(3, 4)

    # ismutable should return true for mutable struct instances
    result1 = ismutable(m)  # true
    result2 = ismutable(i)  # false

    # Return 1 if both assertions pass, 0 otherwise
    @test ((result1 == true && result2 == false) ? 1 : 0) == 1.0
end

true  # Test passed
