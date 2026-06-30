# Filtered generator expression
# collect(x for x in 1:10 if x > 5) = [6, 7, 8, 9, 10]

using Test

@testset "Filtered generator expression" begin
    g = (x for x in 1:10 if x > 5)
    result = collect(g)
    @test (sum(result)) == 40.0

    function generator_filtered_double_4134(x)
        x * 2
    end

    function generator_filtered_is_even_4134(x)
        x % 2 == 0
    end

    lazy = (generator_filtered_double_4134(x) for x in 1:6 if generator_filtered_is_even_4134(x))
    @test lazy isa Base.Generator
    @test !(lazy isa Vector)
    @test collect(lazy) == [4, 8, 12]
    first_step = iterate(lazy)
    @test first_step == (4, 2)
    @test iterate(lazy, first_step[2]) == (8, 4)

    matrix_lazy = (generator_filtered_double_4134(x) for x in [1 2; 3 4] if isodd(x))
    matrix_result = collect(matrix_lazy)
    @test matrix_result == [2, 6]
    @test size(matrix_result) == (2,)

    function generator_filtered_is_false_4134(x)
        false
    end

    empty = collect(generator_filtered_double_4134(x) for x in 1:3 if generator_filtered_is_false_4134(x))
    @test length(empty) == 0

    function generator_filtered_boom_4134(x)
        error("filtered generator predicate should be lazy")
    end

    lazy_error = (generator_filtered_double_4134(x) for x in [1] if generator_filtered_boom_4134(x))
    @test lazy_error isa Base.Generator
    @test_throws ErrorException collect(lazy_error)
end

true  # Test passed
