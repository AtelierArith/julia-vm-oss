# eachrow, eachcol, dropdims (Issue #1946)

using Test

@testset "eachrow" begin
    A = [1.0 2.0 3.0; 4.0 5.0 6.0; 7.0 8.0 9.0]

    # Test iteration via for loop
    row_count = 0
    row_sums = zeros(3)
    for row in eachrow(A)
        row_count = row_count + 1
        row_sums[row_count] = sum(row)
    end
    @test row_count == 3
    @test abs(row_sums[1] - 6.0) < 1e-10   # 1+2+3 = 6
    @test abs(row_sums[2] - 15.0) < 1e-10  # 4+5+6 = 15
    @test abs(row_sums[3] - 24.0) < 1e-10  # 7+8+9 = 24

    # Test length
    @test length(eachrow(A)) == 3

    # 2x2 matrix
    B = [10.0 20.0; 30.0 40.0]
    sums2 = zeros(2)
    idx = 0
    for row in eachrow(B)
        idx = idx + 1
        sums2[idx] = sum(row)
    end
    @test abs(sums2[1] - 30.0) < 1e-10   # 10+20 = 30
    @test abs(sums2[2] - 70.0) < 1e-10   # 30+40 = 70
end

@testset "eachcol" begin
    A = [1.0 2.0 3.0; 4.0 5.0 6.0; 7.0 8.0 9.0]

    # Test iteration via for loop
    col_count = 0
    col_sums = zeros(3)
    for col in eachcol(A)
        col_count = col_count + 1
        col_sums[col_count] = sum(col)
    end
    @test col_count == 3
    @test abs(col_sums[1] - 12.0) < 1e-10  # 1+4+7 = 12
    @test abs(col_sums[2] - 15.0) < 1e-10  # 2+5+8 = 15
    @test abs(col_sums[3] - 18.0) < 1e-10  # 3+6+9 = 18

    # Test length
    @test length(eachcol(A)) == 3

    # 2x2 matrix
    B = [10.0 20.0; 30.0 40.0]
    sums2 = zeros(2)
    idx = 0
    for col in eachcol(B)
        idx = idx + 1
        sums2[idx] = sum(col)
    end
    @test abs(sums2[1] - 40.0) < 1e-10   # 10+30 = 40
    @test abs(sums2[2] - 60.0) < 1e-10   # 20+40 = 60
end

@testset "dropdims" begin
    # Drop dimension 2 from (3, 1) matrix -> 1D vector
    A = reshape([1.0, 2.0, 3.0], 3, 1)
    v = dropdims(A, dims=2)
    @test length(v) == 3
    @test abs(v[1] - 1.0) < 1e-10
    @test abs(v[2] - 2.0) < 1e-10
    @test abs(v[3] - 3.0) < 1e-10

    # Drop dimension 1 from (1, 4) matrix -> 1D vector
    B = reshape([10.0, 20.0, 30.0, 40.0], 1, 4)
    w = dropdims(B, dims=1)
    @test length(w) == 4
    @test abs(w[1] - 10.0) < 1e-10
    @test abs(w[2] - 20.0) < 1e-10
    @test abs(w[3] - 30.0) < 1e-10
    @test abs(w[4] - 40.0) < 1e-10

    # Single element (1, 1) matrix
    C = reshape([42.0], 1, 1)
    u = dropdims(C, dims=1)
    @test length(u) == 1
    @test abs(u[1] - 42.0) < 1e-10

    u2 = dropdims(C, dims=2)
    @test length(u2) == 1
    @test abs(u2[1] - 42.0) < 1e-10
end

true
