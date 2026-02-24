# Operator partial application (Issue #3119)
# Tests that ==(x), >(x), <(x) etc. work as predicate closures

using Test

@testset "Basic operator partial apply with findfirst" begin
    arr = [1, 2, 3, 4, 5]
    @test findfirst(==(3), arr) == 3
    @test findfirst(==(1), arr) == 1
    @test findfirst(==(5), arr) == 5
    @test findfirst(==(6), arr) === nothing
end

@testset "findall with partial apply" begin
    arr = [1, 2, 3, 4, 5]
    @test findall(>(3), arr) == [4, 5]
    @test findall(==(3), arr) == [3]
    @test findall(<(3), arr) == [1, 2]
end

@testset "count with partial apply" begin
    arr = [1, 2, 3, 4, 5]
    @test count(==(3), arr) == 1
    @test count(>(3), arr) == 2
    @test count(<(3), arr) == 2
end

@testset "Parenthesized partial apply" begin
    arr = [1, 2, 3, 4, 5]
    @test findfirst((==(2)), arr) == 2
    @test findlast((>(3)), arr) == 5
    @test findfirst((>=(3)), arr) == 3
end

@testset "Partial apply inside function body" begin
    function find_first_equal(val, collection::Array)
        findfirst(==(val), collection)
    end
    arr = [10, 20, 30, 40, 50]
    @test find_first_equal(30, arr) == 3
    @test find_first_equal(10, arr) == 1
    @test find_first_equal(99, arr) === nothing
end

@testset "findlast with partial apply inside function" begin
    function find_last_greater(val, collection::Array)
        findlast(>(val), collection)
    end
    arr = [10, 20, 30, 40, 50]
    @test find_last_greater(20, arr) == 5
    @test find_last_greater(50, arr) === nothing
    @test find_last_greater(0, arr) == 5
end

true
