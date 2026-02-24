# Test HOF (Higher-Order Function) type inference
# Issue #1640: Enhance type inference for map/filter/reduce
#
# This tests that the compiler correctly infers return types for HOF calls
# at compile-time, enabling type-stable code generation.

using Test

# Helper functions for testing
add1(x) = x + 1
double(x) = x * 2
mysum(a, b) = a + b
myprod(a, b) = a * b

@testset "HOF Type Inference" begin
    # Test reduce type inference
    # reduce(mysum, [1,2,3,4]) should infer Int64 return type
    @testset "reduce type inference" begin
        result = reduce(mysum, [1, 2, 3, 4])
        @test result == 10
        @test isa(result, Int64)

        # reduce with multiplication
        result2 = reduce(myprod, [2, 3, 4])
        @test result2 == 24
        @test isa(result2, Int64)
    end

    # Test foldl type inference (alias for reduce)
    @testset "foldl type inference" begin
        result = foldl(mysum, [1, 2, 3])
        @test result == 6
        @test isa(result, Int64)
    end

    # Test foldr type inference
    @testset "foldr type inference" begin
        result = foldr(mysum, [1, 2, 3])
        @test result == 6
        @test isa(result, Int64)
    end

    # Test map with named function
    # map(add1, [1,2,3]) should infer Array{Int64} at compile-time
    @testset "map with named function" begin
        result = map(add1, [1, 2, 3])
        @test length(result) == 3
        @test result[1] == 2
        @test result[2] == 3
        @test result[3] == 4
    end

    # Test map with lambda function
    @testset "map with lambda" begin
        result = map(x -> x * 2, [1, 2, 3])
        @test length(result) == 3
        @test result[1] == 2
        @test result[2] == 4
        @test result[3] == 6
    end

    # Test filter
    @testset "filter type inference" begin
        result = filter(x -> x > 1, [1, 2, 3])
        @test length(result) == 2
        @test result[1] == 2
        @test result[2] == 3
    end
end

true
