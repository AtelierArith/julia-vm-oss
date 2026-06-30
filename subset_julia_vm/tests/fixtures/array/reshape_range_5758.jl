using Test

# Issue #5758: reshape of a range materializes and reshapes it. Previously failed
# with "reshape: expected Array, got Range". (Julia returns a lazy ReshapedArray;
# sjulia materializes a Matrix — values and display match, which is what we test.)

@testset "reshape(range, dims) (Issue #5758)" begin
    # Column-major fill, varargs dims
    @test reshape(1:6, 2, 3) == [1 3 5; 2 4 6]
    @test reshape(1:6, 3, 2) == [1 4; 2 5; 3 6]

    # Tuple dims
    @test reshape(1:6, (2, 3)) == [1 3 5; 2 4 6]

    # Zero-based and step ranges
    @test reshape(0:5, 2, 3) == [0 2 4; 1 3 5]
    @test reshape(1:2:12, 2, 3) == [1 5 9; 3 7 11]

    # Reshape to a 1×N / N×1
    @test reshape(1:4, 1, 4) == [1 2 3 4]
    @test reshape(1:4, 4, 1) == reshape([1, 2, 3, 4], 4, 1)

    # Result matches reshaping the collected vector
    @test reshape(1:6, 2, 3) == reshape(collect(1:6), 2, 3)

    # Array reshape is unchanged (regression guard)
    @test reshape([1, 2, 3, 4, 5, 6], 2, 3) == [1 3 5; 2 4 6]
end

true
