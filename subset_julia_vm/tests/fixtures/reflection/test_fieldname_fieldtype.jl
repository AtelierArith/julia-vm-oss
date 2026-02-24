# Test fieldname and fieldtype functions (Issue #352)
# Tests that fieldname returns Symbol and fieldtype returns Type.

using Test

struct FieldTestPoint
    x::Float64
    y::Float64
end

struct FieldTestComplex
    real::Float64
    imag::Float64
    name::String
end

@testset "fieldname/fieldtype: get field name as Symbol and type (Issue #352)" begin



    # Test 1: fieldname returns Symbol
    @assert fieldname(FieldTestPoint, 1) == :x
    @assert fieldname(FieldTestPoint, 2) == :y

    # Test 2: fieldtype returns Type (use string comparison for DataType)
    @assert isequal(string(fieldtype(FieldTestPoint, 1)), "Float64")
    @assert isequal(string(fieldtype(FieldTestPoint, 2)), "Float64")

    # Test 3: Works with multiple field types
    @assert fieldname(FieldTestComplex, 1) == :real
    @assert fieldname(FieldTestComplex, 2) == :imag
    @assert fieldname(FieldTestComplex, 3) == :name

    @assert isequal(string(fieldtype(FieldTestComplex, 1)), "Float64")
    @assert isequal(string(fieldtype(FieldTestComplex, 2)), "Float64")
    @assert isequal(string(fieldtype(FieldTestComplex, 3)), "String")

    # Test 4: Type of return values
    @assert isa(fieldname(FieldTestPoint, 1), Symbol)
    @assert isa(fieldtype(FieldTestPoint, 1), DataType)

    @test (true)
end

true  # Test passed
