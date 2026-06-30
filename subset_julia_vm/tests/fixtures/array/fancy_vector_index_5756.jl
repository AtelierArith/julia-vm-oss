using Test

# Issue #5756: indexing with a vector literal — arr[[1,3,5]] (fancy / vector
# indexing) — was lowered as multi-dimensional arr[1,3,5] and raised a dimension
# mismatch. A vector literal index is a single (fancy) index.

@testset "fancy vector-literal indexing (Issue #5756)" begin
    # 1D fancy indexing with a vector literal
    @test [10, 20, 30, 40, 50][[1, 3, 5]] == [10, 30, 50]
    @test [10, 20, 30, 40, 50][[2, 4]] == [20, 40]
    a = [1, 2, 3, 4, 5]
    @test a[[1, 5]] == [1, 5]
    @test a[[3]] == [3]

    # Order / repetition is preserved
    @test [10, 20, 30][[3, 1, 2]] == [30, 10, 20]
    @test [10, 20, 30][[1, 1, 2]] == [10, 10, 20]

    # Logical (Bool vector literal) indexing
    @test [10, 20, 30][[true, false, true]] == [10, 30]

    # 2D fancy indexing with vector literals on each dimension
    m = [1 2 3; 4 5 6]
    @test m[[1, 2], [1, 3]] == [1 3; 4 6]

    # A vector *variable* index already worked — keep it consistent
    k = [1, 3, 5]
    @test [10, 20, 30, 40, 50][k] == [10, 30, 50]

    # Genuine multi-dimensional indexing on a 1D array is still an error
    @test_throws Exception [1, 2, 3, 4][1, 3]
end

true
