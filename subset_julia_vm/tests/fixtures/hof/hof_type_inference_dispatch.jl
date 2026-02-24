# HOF type inference dispatch tests (Issue #1671)
# Verify that functions passed as arguments to HOFs are correctly
# dispatched regardless of how they are referenced (named, lambda, variable).

using Test

# Helper functions defined OUTSIDE @testset (scope rule)
triple(x) = x * 3
ispos(x) = x > 0
double(x) = x * 2

@testset "Named function passed to map" begin
    @test map(triple, [1, 2, 3]) == [3, 6, 9]
end

@testset "Lambda passed to map" begin
    @test map(x -> x * 2, [1, 2, 3]) == [2, 4, 6]
end

@testset "Named function passed to filter" begin
    @test filter(ispos, [-1, 0, 1, 2]) == [1, 2]
end

@testset "Lambda passed to filter" begin
    @test filter(x -> x > 0, [-1, 0, 1, 2]) == [1, 2]
end

@testset "HOF chaining: map on filter result" begin
    @test map(triple, filter(ispos, [-1, 0, 1, 2])) == [3, 6]
end

@testset "HOF chaining: filter on map result" begin
    @test filter(ispos, map(x -> x - 3, [1, 2, 3, 4, 5])) == [1, 2]
end

@testset "Named function with reduce" begin
    mymax(a, b) = a > b ? a : b
    @test reduce(mymax, [3, 1, 4, 1, 5]) == 5
end

@testset "Lambda with reduce" begin
    @test reduce((a, b) -> a + b, [1, 2, 3, 4]) == 10
end

true
