# Test LinRange iteration
# Issue #944: LinRange iteration fails in mandelbrot_grid sample

using Test

@testset "LinRange iteration" begin
    # Basic LinRange iteration
    xs = range(-2.0, 1.0; length=5)
    result = Float64[]
    for x in xs
        push!(result, x)
    end
    @test length(result) == 5
    @test abs(result[1] - (-2.0)) < 1e-10
    @test abs(result[2] - (-1.25)) < 1e-10
    @test abs(result[3] - (-0.5)) < 1e-10
    @test abs(result[4] - 0.25) < 1e-10
    @test abs(result[5] - 1.0) < 1e-10

    # LinRange with length=1 (single element)
    xs_single = range(5.0, 5.0; length=1)
    result_single = Float64[]
    for x in xs_single
        push!(result_single, x)
    end
    @test length(result_single) == 1
    @test abs(result_single[1] - 5.0) < 1e-10

    # LinRange with length=2 (start and stop only)
    xs_two = range(0.0, 10.0; length=2)
    result_two = Float64[]
    for x in xs_two
        push!(result_two, x)
    end
    @test length(result_two) == 2
    @test abs(result_two[1] - 0.0) < 1e-10
    @test abs(result_two[2] - 10.0) < 1e-10

    # LinRange with integer-like values
    xs_int = range(0.0, 10.0; length=11)
    result_int = Float64[]
    for x in xs_int
        push!(result_int, x)
    end
    @test length(result_int) == 11
    for i in 1:11
        @test abs(result_int[i] - Float64(i - 1)) < 1e-10
    end
end

true
