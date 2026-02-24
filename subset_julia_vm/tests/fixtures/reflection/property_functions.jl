# Test property functions: getproperty, setproperty!, propertynames
# Issue #1450: Implement property functions
# Issue #1451: Implement setfield! builtin

using Test

struct ImmutablePoint
    x::Float64
    y::Float64
end

mutable struct MutablePoint
    x::Float64
    y::Float64
end

@testset "Property functions" begin
    @testset "getproperty" begin
        p = ImmutablePoint(1.0, 2.0)

        # getproperty should return field values
        @test getproperty(p, :x) == 1.0
        @test getproperty(p, :y) == 2.0

        # should be equivalent to p.x
        @test p.x == getproperty(p, :x)
        @test p.y == getproperty(p, :y)
    end

    @testset "setfield!" begin
        # Test setfield! builtin function
        p = MutablePoint(1.0, 2.0)

        # setfield! by Symbol
        setfield!(p, :x, 3.0)
        @test p.x == 3.0

        # setfield! by index
        setfield!(p, 2, 4.0)
        @test p.y == 4.0

        # setfield! returns the assigned value
        result = setfield!(p, :x, 5.0)
        @test result == 5.0
        @test p.x == 5.0
    end

    @testset "setproperty!" begin
        p = MutablePoint(1.0, 2.0)

        # setproperty! should modify field values
        setproperty!(p, :x, 3.0)
        @test p.x == 3.0

        setproperty!(p, :y, 4.0)
        @test p.y == 4.0

        # should be equivalent to p.x = value
        p.x = 5.0
        @test getproperty(p, :x) == 5.0
    end

    @testset "setproperty! with type conversion" begin
        p = MutablePoint(1.0, 2.0)

        # setproperty! should convert Int to Float64
        setproperty!(p, :x, 10)
        @test p.x == 10.0
        @test typeof(p.x) == Float64
    end

    @testset "propertynames" begin
        p = ImmutablePoint(1.0, 2.0)

        # propertynames should return tuple of field names
        pnames = propertynames(p)
        @test length(pnames) == 2
        # Note: fieldnames returns strings in SubsetJuliaVM, so convert to Symbol for comparison
        @test Symbol(pnames[1]) == :x
        @test Symbol(pnames[2]) == :y
    end

    @testset "hasproperty with propertynames" begin
        p = ImmutablePoint(1.0, 2.0)

        # hasproperty should work with property names
        @test hasproperty(p, :x) == true
        @test hasproperty(p, :y) == true
        @test hasproperty(p, :z) == false
    end
end

true
