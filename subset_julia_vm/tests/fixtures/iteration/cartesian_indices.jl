# CartesianIndices iteration test
# Tests column-major iteration order

using Test

@testset "CartesianIndices iteration in column-major order" begin

    # Test 2x2 grid
    result_2x2 = zeros(0)
    for I in CartesianIndices((2, 2))
        # Store i + j*10 to uniquely identify each (i, j) pair
        push!(result_2x2, Float64(I.I[1] + I.I[2] * 10))
    end
    # Column-major order: (1,1)=11, (2,1)=12, (1,2)=21, (2,2)=22
    test1 = result_2x2[1] == 11.0 && result_2x2[2] == 12.0 && result_2x2[3] == 21.0 && result_2x2[4] == 22.0

    # Test 3x2 grid
    result_3x2 = zeros(0)
    for I in CartesianIndices((3, 2))
        push!(result_3x2, Float64(I.I[1] + I.I[2] * 10))
    end
    # Verify 6 elements: (1,1)=11, (2,1)=12, (3,1)=13, (1,2)=21, (2,2)=22, (3,2)=23
    test2 = length(result_3x2) == 6 && result_3x2[1] == 11.0 && result_3x2[6] == 23.0

    # Test 1D case
    result_1d = zeros(0)
    for I in CartesianIndices((4,))
        push!(result_1d, Float64(I.I[1]))
    end
    test3 = length(result_1d) == 4 && result_1d[1] == 1.0 && result_1d[4] == 4.0

    # Test empty (zero dimension)
    count_empty = 0
    for I in CartesianIndices((0, 2))
        count_empty = count_empty + 1
    end
    test4 = count_empty == 0

    # All tests must pass
    @test (test1 && test2 && test3 && test4)
end

true  # Test passed
