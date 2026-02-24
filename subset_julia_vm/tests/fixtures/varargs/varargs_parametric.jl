# Test varargs parameters with parametric type annotations (Issue #1685)
# Typed varargs like (xs::Vector{Int64}...) should work correctly

using Test

# Helper functions defined outside testset
# Parametric type varargs - accepts multiple Vector arguments
function concat_vectors(vs::Vector{Int64}...)
    result = Int64[]
    for v in vs
        for x in v
            push!(result, x)
        end
    end
    result
end

function count_vectors(vs::Vector{Int64}...)
    length(vs)
end

function sum_first_elements(vs::Vector{Int64}...)
    total = 0
    for v in vs
        if length(v) > 0
            total += v[1]
        end
    end
    total
end

@testset "Parametric type varargs parameters" begin
    # Test with no arguments
    @test count_vectors() == 0

    # Test with single Vector argument
    @test count_vectors([1, 2, 3]) == 1

    # Test with multiple Vector arguments
    @test count_vectors([1], [2], [3]) == 3

    # Test vector concatenation
    @test concat_vectors([1, 2], [3, 4]) == [1, 2, 3, 4]
    @test concat_vectors([1], [2], [3]) == [1, 2, 3]

    # Test sum of first elements
    @test sum_first_elements([10, 20], [30, 40]) == 40
    @test sum_first_elements([1], [2], [3]) == 6
end

true
