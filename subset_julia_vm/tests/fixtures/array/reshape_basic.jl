# reshape: return a new array structure with changed dimensions

using Test

@testset "reshape returns a separate array structure" begin
    arr = [1, 2, 3, 4, 5, 6]
    mat = reshape(arr, 2, 3)

    @test length(mat) == 6
    @test size(mat) == (2, 3)
    @test mat[2, 3] == 6

    @test size(arr) == (6,)
    @test arr[6] == 6

    mat[1, 2] = 99
    @test arr[3] == 99

    arr[6] = 42
    @test mat[2, 3] == 42
    @test repr(mat) == "[1 99 5; 2 4 42]"
    @test mat == [1 99 5; 2 4 42]
end

true  # Test passed
