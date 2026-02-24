# Test equality comparison for DataType values

using Test

@testset "DataType equality" begin
    # Same types should be equal
    @test Int64 == Int64
    @test Float64 == Float64
    @test String == String
    @test Bool == Bool

    # Different types should not be equal
    @test Int64 != Float64
    @test Int64 != Int32
    @test String != Symbol
    @test Bool != Int64

    # Type stored in variable
    T = Int64
    @test T == Int64
    @test T != Float64

    # Array and abstract types
    @test Array == Array
    @test Number == Number
    @test Integer == Integer
end

true
