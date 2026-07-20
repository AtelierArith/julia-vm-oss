# Aggregated concat-safe @testset fixtures (Issue #9671 Phase 3 pilot).
# Each block below is one former standalone fixture, verbatim except its
# `using Test` / trailing `true` were hoisted. @testset names (with their
# original Issue numbers) are preserved, and the #9360 @testset gate still
# detects any per-@testset failure. Source fixture in each banner.
using Test

# ===== source: array/adjoint_transpose.jl =====
# Test Pure Julia adjoint and transpose for arrays


@testset "Array transpose and adjoint (Pure Julia)" begin
    # 1D vector transpose -> row vector
    v = [1.0, 2.0, 3.0]
    vt = transpose(v)
    @test size(vt) == (1, 3)
    @test vt[1, 1] == 1.0
    @test vt[1, 2] == 2.0
    @test vt[1, 3] == 3.0

    # 2D matrix transpose
    A = [1.0 2.0; 3.0 4.0]
    At = transpose(A)
    @test size(At) == (2, 2)
    @test At[1, 1] == 1.0
    @test At[1, 2] == 3.0
    @test At[2, 1] == 2.0
    @test At[2, 2] == 4.0

    # adjoint for real matrix (same as transpose)
    Ad = adjoint(A)
    @test size(Ad) == (2, 2)
    @test Ad[1, 1] == 1.0
    @test Ad[1, 2] == 3.0
    @test Ad[2, 1] == 2.0
    @test Ad[2, 2] == 4.0

    # 1D vector adjoint -> row vector (conjugated)
    va = adjoint(v)
    @test size(va) == (1, 3)
    @test va[1, 1] == 1.0
    @test va[1, 2] == 2.0
    @test va[1, 3] == 3.0
end

# ===== source: array/eachrow_eachcol_dropdims.jl =====
# eachrow, eachcol, dropdims (Issue #1946)


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

# ===== source: array/matrix_rotation.jl =====
# Test rotl90, rotr90, rot180 matrix rotation functions (Issue #1879)


@testset "rotl90 basic" begin
    # [1 2; 3 4] rotated left 90 degrees -> [2 4; 1 3]
    mat = [1.0 2.0; 3.0 4.0]
    r = rotl90(mat)
    @test r[1, 1] == 2.0
    @test r[1, 2] == 4.0
    @test r[2, 1] == 1.0
    @test r[2, 2] == 3.0
end

@testset "rotr90 basic" begin
    # [1 2; 3 4] rotated right 90 degrees -> [3 1; 4 2]
    mat = [1.0 2.0; 3.0 4.0]
    r = rotr90(mat)
    @test r[1, 1] == 3.0
    @test r[1, 2] == 1.0
    @test r[2, 1] == 4.0
    @test r[2, 2] == 2.0
end

@testset "rot180 basic" begin
    # [1 2; 3 4] rotated 180 degrees -> [4 3; 2 1]
    mat = [1.0 2.0; 3.0 4.0]
    r = rot180(mat)
    @test r[1, 1] == 4.0
    @test r[1, 2] == 3.0
    @test r[2, 1] == 2.0
    @test r[2, 2] == 1.0
end

@testset "rotl90 then rotr90 is identity" begin
    mat = [1.0 2.0; 3.0 4.0]
    r = rotr90(rotl90(mat))
    @test r[1, 1] == 1.0
    @test r[1, 2] == 2.0
    @test r[2, 1] == 3.0
    @test r[2, 2] == 4.0
end

@testset "rot180 twice is identity" begin
    mat = [1.0 2.0; 3.0 4.0]
    r = rot180(rot180(mat))
    @test r[1, 1] == 1.0
    @test r[1, 2] == 2.0
    @test r[2, 1] == 3.0
    @test r[2, 2] == 4.0
end

# ===== source: array/stack_selectdim.jl =====
# stack, selectdim (Issue #1942)


@testset "stack" begin
    # Stack two 1D arrays into a 2-column matrix
    a = [1.0, 2.0, 3.0]
    b = [4.0, 5.0, 6.0]
    M = stack([a, b])
    @test size(M) == (3, 2)
    @test abs(M[1, 1] - 1.0) < 1e-10
    @test abs(M[2, 1] - 2.0) < 1e-10
    @test abs(M[3, 1] - 3.0) < 1e-10
    @test abs(M[1, 2] - 4.0) < 1e-10
    @test abs(M[2, 2] - 5.0) < 1e-10
    @test abs(M[3, 2] - 6.0) < 1e-10

    # Stack three 1D arrays
    c = [7.0, 8.0, 9.0]
    M3 = stack([a, b, c])
    @test size(M3) == (3, 3)
    @test abs(M3[1, 3] - 7.0) < 1e-10
    @test abs(M3[2, 3] - 8.0) < 1e-10
    @test abs(M3[3, 3] - 9.0) < 1e-10

    # Stack single array
    M1 = stack([a])
    @test size(M1) == (3, 1)
    @test abs(M1[1, 1] - 1.0) < 1e-10
    @test abs(M1[3, 1] - 3.0) < 1e-10

    # Stack 2-element arrays
    x = [10.0, 20.0]
    y = [30.0, 40.0]
    M2 = stack([x, y])
    @test size(M2) == (2, 2)
    @test abs(M2[1, 1] - 10.0) < 1e-10
    @test abs(M2[2, 2] - 40.0) < 1e-10
end

@testset "selectdim" begin
    A = [1.0 2.0 3.0; 4.0 5.0 6.0; 7.0 8.0 9.0]

    # Select row 1 (dimension 1, index 1)
    r1 = selectdim(A, 1, 1)
    @test length(r1) == 3
    @test abs(r1[1] - 1.0) < 1e-10
    @test abs(r1[2] - 2.0) < 1e-10
    @test abs(r1[3] - 3.0) < 1e-10

    # Select row 2
    r2 = selectdim(A, 1, 2)
    @test abs(r2[1] - 4.0) < 1e-10
    @test abs(r2[2] - 5.0) < 1e-10
    @test abs(r2[3] - 6.0) < 1e-10

    # Select row 3
    r3 = selectdim(A, 1, 3)
    @test abs(r3[1] - 7.0) < 1e-10

    # Select column 1 (dimension 2, index 1)
    c1 = selectdim(A, 2, 1)
    @test length(c1) == 3
    @test abs(c1[1] - 1.0) < 1e-10
    @test abs(c1[2] - 4.0) < 1e-10
    @test abs(c1[3] - 7.0) < 1e-10

    # Select column 2
    c2 = selectdim(A, 2, 2)
    @test abs(c2[1] - 2.0) < 1e-10
    @test abs(c2[2] - 5.0) < 1e-10
    @test abs(c2[3] - 8.0) < 1e-10

    # Select column 3
    c3 = selectdim(A, 2, 3)
    @test abs(c3[1] - 3.0) < 1e-10
    @test abs(c3[2] - 6.0) < 1e-10
    @test abs(c3[3] - 9.0) < 1e-10

    # 2x2 matrix
    B = [10.0 20.0; 30.0 40.0]
    r = selectdim(B, 1, 2)
    @test abs(r[1] - 30.0) < 1e-10
    @test abs(r[2] - 40.0) < 1e-10
    c = selectdim(B, 2, 1)
    @test abs(c[1] - 10.0) < 1e-10
    @test abs(c[2] - 30.0) < 1e-10
end

true
