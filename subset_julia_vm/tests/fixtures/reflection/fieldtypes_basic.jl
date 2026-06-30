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

    @test fieldtypes(LineNumberNode)[1] === Int64
    @test string(fieldtypes(LineNumberNode)[2]) == "Union{Nothing, Symbol}"
    @test fieldtypes(Expr)[1] === Symbol
    @test fieldtypes(Expr)[2] === Vector{Any}
    @test fieldtypes(QuoteNode)[1] === Any
    @test fieldtypes(GlobalRef)[1] === Module
    @test fieldtypes(GlobalRef)[2] === Symbol
    @test string(fieldtypes(GlobalRef)[3]) == "Core.Binding"
end

true  # Test passed
