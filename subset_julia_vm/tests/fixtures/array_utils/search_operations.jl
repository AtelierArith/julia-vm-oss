using Test

# Note: operator partial application syntax ==(x), >(x) is not yet supported (Issue #3119).
# Use explicit lambdas instead: x -> x == val, x -> x > val

@testset "Array search operations" begin
    @testset "findfirst" begin
        @test findfirst(x -> x == 3, [1, 2, 3, 4, 5]) == 3
        @test findfirst(x -> x == 3, [3, 2, 3]) == 1  # first occurrence
        @test findfirst(x -> x > 3, [1, 2, 3, 4, 5]) == 4
        @test isnothing(findfirst(x -> x == 99, [1, 2, 3]))
    end

    @testset "findlast" begin
        @test findlast(x -> x == 3, [1, 2, 3, 4, 3]) == 5  # last occurrence
        @test findlast(x -> x == 3, [3, 2, 1]) == 1
        @test isnothing(findlast(x -> x == 99, [1, 2, 3]))
    end

    @testset "findall" begin
        @test findall(x -> x == 2, [1, 2, 3, 2, 2]) == [2, 4, 5]
        @test findall(x -> x > 3, [1, 2, 3, 4, 5]) == [4, 5]
        @test isempty(findall(x -> x == 99, [1, 2, 3]))
    end

    @testset "count" begin
        @test count(x -> x == 2, [1, 2, 3, 2, 2]) == 3
        @test count(x -> x > 3, [1, 2, 3, 4, 5]) == 2
        @test count(x -> x == 99, [1, 2, 3]) == 0
    end

    @testset "filter" begin
        @test filter(x -> x > 3, [1, 2, 3, 4, 5]) == [4, 5]
        @test filter(iseven, [1, 2, 3, 4, 6]) == [2, 4, 6]
        @test isempty(filter(x -> x == 5, [1, 2, 3]))
    end
end

true
