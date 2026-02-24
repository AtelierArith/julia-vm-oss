# Test isa() with primitive type identifiers (Issue #1267)
# Verifies that isa(x, Float64), isa(x, Int64), etc. work correctly
# without causing struct_ops errors

using Test

@testset "isa with primitive type identifiers" begin
    # Float64
    @test isa(3.14, Float64) == true
    @test isa(1, Float64) == false
    @test isa("hello", Float64) == false

    # Int64
    @test isa(42, Int64) == true
    @test isa(3.14, Int64) == false
    @test isa("hello", Int64) == false

    # Bool
    @test isa(true, Bool) == true
    @test isa(false, Bool) == true
    @test isa(1, Bool) == false

    # String
    @test isa("hello", String) == true
    @test isa(42, String) == false

    # Char
    @test isa('a', Char) == true
    @test isa("a", Char) == false

    # Nothing
    @test isa(nothing, Nothing) == true
    @test isa(0, Nothing) == false
end

@testset "isa in conditional expressions" begin
    # Test using isa in if conditions - the original bug scenario
    function format_value(x)
        if isa(x, Float64)
            return "float"
        elseif isa(x, Int64)
            return "int"
        elseif isa(x, String)
            return "string"
        else
            return "other"
        end
    end

    @test format_value(3.14) == "float"
    @test format_value(42) == "int"
    @test format_value("hello") == "string"
    @test format_value(true) == "other"  # Bool is not Int64
end

@testset "isa with abstract types" begin
    # Test with abstract type hierarchy
    @test isa(42, Integer) == true
    @test isa(42, Number) == true
    @test isa(3.14, AbstractFloat) == true
    @test isa(3.14, Number) == true
    @test isa("hello", AbstractString) == true
end

true
