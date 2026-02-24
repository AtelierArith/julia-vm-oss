# Test permutedims! in-place dimension permutation

using Test

@testset "permutedims! 2D transpose" begin
    src = [1.0 2.0; 3.0 4.0]
    dest = zeros(2, 2)
    permutedims!(dest, src, (2, 1))
    @test dest[1, 1] == 1.0
    @test dest[1, 2] == 3.0
    @test dest[2, 1] == 2.0
    @test dest[2, 2] == 4.0
end

@testset "permutedims! 2D identity" begin
    src = [1.0 2.0; 3.0 4.0]
    dest = zeros(2, 2)
    permutedims!(dest, src, (1, 2))
    @test dest[1, 1] == 1.0
    @test dest[1, 2] == 2.0
    @test dest[2, 1] == 3.0
    @test dest[2, 2] == 4.0
end

@testset "permutedims! 2D rectangular" begin
    src = [1.0 2.0 3.0; 4.0 5.0 6.0]
    dest = zeros(3, 2)
    permutedims!(dest, src, (2, 1))
    @test dest[1, 1] == 1.0
    @test dest[2, 1] == 2.0
    @test dest[3, 1] == 3.0
    @test dest[1, 2] == 4.0
    @test dest[2, 2] == 5.0
    @test dest[3, 2] == 6.0
end

@testset "permutedims! 3D array" begin
    src = zeros(2, 3, 4)
    for i in 1:2
        for j in 1:3
            for k in 1:4
                src[i, j, k] = Float64(100 * i + 10 * j + k)
            end
        end
    end
    dest = zeros(3, 2, 4)
    permutedims!(dest, src, (2, 1, 3))
    # src[1,2,3] = 123.0, should be at dest[2,1,3]
    @test dest[2, 1, 3] == 123.0
    # src[2,3,4] = 234.0, should be at dest[3,2,4]
    @test dest[3, 2, 4] == 234.0
end

true
