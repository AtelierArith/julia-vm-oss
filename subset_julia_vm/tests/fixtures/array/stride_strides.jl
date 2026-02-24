# stride and strides - Memory stride for column-major arrays (Issue #2157)

using Test

@testset "stride - 1D vector" begin
    v = [1.0, 2.0, 3.0, 4.0]
    @test stride(v, 1) == 1
    # Beyond ndims: stride equals total length
    @test stride(v, 2) == 4
end

@testset "stride - 2D matrix" begin
    A = [1.0 2.0 3.0; 4.0 5.0 6.0]  # 2×3 matrix
    @test stride(A, 1) == 1
    @test stride(A, 2) == 2   # size(A, 1) = 2
    # Beyond ndims: stride equals total elements
    @test stride(A, 3) == 6   # 2 * 3
end

@testset "stride - 3D array" begin
    B = zeros(2, 3, 4)
    @test stride(B, 1) == 1
    @test stride(B, 2) == 2   # size(B, 1) = 2
    @test stride(B, 3) == 6   # size(B, 1) * size(B, 2) = 2 * 3
end

@testset "strides - 1D vector" begin
    v = [1.0, 2.0, 3.0]
    s = strides(v)
    @test s[1] == 1
end

@testset "strides - 2D matrix" begin
    A = [1.0 2.0 3.0; 4.0 5.0 6.0]  # 2×3 matrix
    s = strides(A)
    @test s[1] == 1
    @test s[2] == 2
end

@testset "strides - 3D array" begin
    B = zeros(2, 3, 4)
    s = strides(B)
    @test s[1] == 1
    @test s[2] == 2
    @test s[3] == 6
end

true
