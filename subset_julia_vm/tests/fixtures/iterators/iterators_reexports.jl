# Iterators module re-exports: filter, map, reverse, accumulate,
# takewhile, dropwhile, flatmap (Issue #2159)
# These functions are available in Base and should be accessible via Iterators.* namespace

using Test

@testset "Iterators.filter" begin
    result = collect(Iterators.filter(x -> x > 3, [1, 2, 3, 4, 5]))
    @test result == [4, 5]
end

@testset "Iterators.map" begin
    result = collect(Iterators.map(x -> x * 2, [1, 2, 3]))
    @test result == [2, 4, 6]
end

@testset "Iterators.reverse" begin
    result = collect(Iterators.reverse([1, 2, 3, 4]))
    @test result == [4, 3, 2, 1]
end

@testset "Iterators.accumulate" begin
    result = Iterators.accumulate(+, [1, 2, 3, 4])
    @test result == [1, 3, 6, 10]
end

@testset "Iterators.takewhile" begin
    result = collect(Iterators.takewhile(x -> x < 4, [1, 2, 3, 5, 1]))
    @test result == [1, 2, 3]
end

@testset "Iterators.dropwhile" begin
    result = collect(Iterators.dropwhile(x -> x < 4, [1, 2, 3, 5, 1]))
    @test result == [5, 1]
end

@testset "Iterators.flatmap" begin
    result = collect(Iterators.flatmap(x -> [x, x*10], [1, 2, 3]))
    @test result == [1, 10, 2, 20, 3, 30]
end

true
