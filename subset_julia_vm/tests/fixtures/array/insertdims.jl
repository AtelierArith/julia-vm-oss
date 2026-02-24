# insertdims - Insert singleton dimensions (Issue #2153)
# Inverse of dropdims. Based on Julia's base/abstractarraymath.jl

using Test

@testset "insertdims - 1D vector" begin
    v = [1.0, 2.0, 3.0]

    # dims=1: vector -> 1×3 row matrix
    row = insertdims(v; dims=1)
    @test size(row, 1) == 1
    @test size(row, 2) == 3
    @test row[1, 1] == 1.0
    @test row[1, 2] == 2.0
    @test row[1, 3] == 3.0

    # dims=2: vector -> 3×1 column matrix
    col = insertdims(v; dims=2)
    @test size(col, 1) == 3
    @test size(col, 2) == 1
    @test col[1, 1] == 1.0
    @test col[2, 1] == 2.0
    @test col[3, 1] == 3.0
end

@testset "insertdims - 2D matrix" begin
    A = [1.0 2.0 3.0; 4.0 5.0 6.0]  # 2×3 matrix

    # dims=3: 2×3 -> 2×3×1 array
    B = insertdims(A; dims=3)
    @test ndims(B) == 3
    @test size(B, 1) == 2
    @test size(B, 2) == 3
    @test size(B, 3) == 1
end

@testset "insertdims - roundtrip with dropdims" begin
    v = [10.0, 20.0, 30.0]

    # insertdims then dropdims should give back the original
    row = insertdims(v; dims=1)
    v_back = dropdims(row; dims=1)
    @test length(v_back) == 3
    @test v_back[1] == 10.0
    @test v_back[2] == 20.0
    @test v_back[3] == 30.0

    col = insertdims(v; dims=2)
    v_back2 = dropdims(col; dims=2)
    @test length(v_back2) == 3
    @test v_back2[1] == 10.0
    @test v_back2[2] == 20.0
    @test v_back2[3] == 30.0
end

true
