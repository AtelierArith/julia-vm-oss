# Test @something and @coalesce macros

using Test

@testset "@something and @coalesce macros" begin
    # @something tests - returns first non-nothing value
    @test @something(nothing, 42) == 42
    @test @something(1, 2, 3) == 1
    @test @something(nothing, nothing, 99) == 99

    # @coalesce tests - returns first non-missing value
    @test @coalesce(missing, 42) == 42
    @test @coalesce(1, 2, 3) == 1
    @test @coalesce(missing, missing, 99) == 99

    # Test with expressions
    x = nothing
    @test @something(x, 100) == 100

    y = missing
    @test @coalesce(y, 200) == 200
end

true
