# cat, eachslice, mapslices (Issue #1952)

using Test

@testset "cat dims=1 (vertical)" begin
    # 2D matrices
    A = [1.0 2.0; 3.0 4.0]
    B = [5.0 6.0; 7.0 8.0]
    C = cat(A, B; dims=1)
    @test size(C, 1) == 4
    @test size(C, 2) == 2
    @test abs(C[1, 1] - 1.0) < 1e-10
    @test abs(C[2, 1] - 3.0) < 1e-10
    @test abs(C[3, 1] - 5.0) < 1e-10
    @test abs(C[4, 1] - 7.0) < 1e-10
    @test abs(C[3, 2] - 6.0) < 1e-10
    @test abs(C[4, 2] - 8.0) < 1e-10

    # 1D arrays
    x = [1.0, 2.0, 3.0]
    y = [4.0, 5.0]
    z = cat(x, y; dims=1)
    @test length(z) == 5
    @test abs(z[1] - 1.0) < 1e-10
    @test abs(z[4] - 4.0) < 1e-10
    @test abs(z[5] - 5.0) < 1e-10
end

@testset "cat dims=2 (horizontal)" begin
    # 2D matrices
    A = [1.0 2.0; 3.0 4.0]
    B = [5.0 6.0; 7.0 8.0]
    D = cat(A, B; dims=2)
    @test size(D, 1) == 2
    @test size(D, 2) == 4
    @test abs(D[1, 1] - 1.0) < 1e-10
    @test abs(D[1, 3] - 5.0) < 1e-10
    @test abs(D[2, 4] - 8.0) < 1e-10

    # 1D arrays as columns
    x = [1.0, 2.0, 3.0]
    y = [4.0, 5.0, 6.0]
    M = cat(x, y; dims=2)
    @test size(M, 1) == 3
    @test size(M, 2) == 2
    @test abs(M[1, 1] - 1.0) < 1e-10
    @test abs(M[1, 2] - 4.0) < 1e-10
    @test abs(M[3, 2] - 6.0) < 1e-10
end

@testset "eachslice dims=1 (rows)" begin
    A = [1.0 2.0 3.0; 4.0 5.0 6.0]
    row_sums = Float64[]
    for row in eachslice(A; dims=1)
        push!(row_sums, sum(row))
    end
    @test length(row_sums) == 2
    @test abs(row_sums[1] - 6.0) < 1e-10   # 1+2+3
    @test abs(row_sums[2] - 15.0) < 1e-10  # 4+5+6
end

@testset "eachslice dims=2 (columns)" begin
    A = [1.0 2.0 3.0; 4.0 5.0 6.0]
    col_sums = Float64[]
    for col in eachslice(A; dims=2)
        push!(col_sums, sum(col))
    end
    @test length(col_sums) == 3
    @test abs(col_sums[1] - 5.0) < 1e-10   # 1+4
    @test abs(col_sums[2] - 7.0) < 1e-10   # 2+5
    @test abs(col_sums[3] - 9.0) < 1e-10   # 3+6
end

@testset "mapslices dims=1 (columns)" begin
    A = [1.0 2.0 3.0; 4.0 5.0 6.0]
    # Sum each column
    col_sums = mapslices(sum, A; dims=1)
    @test length(col_sums) == 3
    @test abs(col_sums[1] - 5.0) < 1e-10   # 1+4
    @test abs(col_sums[2] - 7.0) < 1e-10   # 2+5
    @test abs(col_sums[3] - 9.0) < 1e-10   # 3+6
end

@testset "mapslices dims=2 (rows)" begin
    A = [1.0 2.0 3.0; 4.0 5.0 6.0]
    # Sum each row
    row_sums = mapslices(sum, A; dims=2)
    @test length(row_sums) == 2
    @test abs(row_sums[1] - 6.0) < 1e-10   # 1+2+3
    @test abs(row_sums[2] - 15.0) < 1e-10  # 4+5+6
end

true
