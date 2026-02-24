# stack, selectdim (Issue #1942)

using Test

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
