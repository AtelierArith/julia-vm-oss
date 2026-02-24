# Test type aliases with const (Issue #465)

using Test

# Type aliases must be outside @testset
const MyInt = Int64
const MyFloat = Float64

@testset "Type aliases" begin
    # Test 1: Type alias equality - aliases resolve to the same type
    @test MyInt === Int64
    @test MyFloat === Float64

    # Test 2: Type aliases can be used in typeof comparisons
    x = 42
    @test typeof(x) == MyInt

    y = 3.14
    @test typeof(y) == MyFloat

    # Test 3: Type aliases work with isa checks
    @test 100 isa MyInt
    @test 2.5 isa MyFloat
end

true
