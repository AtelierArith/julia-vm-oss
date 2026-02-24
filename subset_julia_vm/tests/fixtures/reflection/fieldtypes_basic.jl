# Test fieldtypes - tuple of field types

using Test

struct TestPoint
    x::Float64
    y::Float64
end

struct TestPerson
    name::String
    age::Int64
    height::Float64
end

@testset "fieldtypes - tuple of field types (length returns Int64)" begin


    # fieldtypes returns a tuple of types
    types_point = fieldtypes(TestPoint)
    types_person = fieldtypes(TestPerson)

    # Check the number of types matches
    result = length(types_point) + length(types_person)  # 2 + 3 = 5
    @test (result) == 5
end

true  # Test passed
