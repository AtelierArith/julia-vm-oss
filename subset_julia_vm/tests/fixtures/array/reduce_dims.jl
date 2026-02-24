# Test sum, prod, maximum, minimum, extrema with dims keyword argument

using Test

A = [1.0 2.0 3.0; 4.0 5.0 6.0; 7.0 8.0 9.0]

@testset "sum with dims" begin
    # dims=1: sum each column → 1×3 result
    S1 = sum(A; dims=1)
    @test S1[1, 1] == 12.0  # 1+4+7
    @test S1[1, 2] == 15.0  # 2+5+8
    @test S1[1, 3] == 18.0  # 3+6+9

    # dims=2: sum each row → 3×1 result
    S2 = sum(A; dims=2)
    @test S2[1, 1] == 6.0   # 1+2+3
    @test S2[2, 1] == 15.0  # 4+5+6
    @test S2[3, 1] == 24.0  # 7+8+9
end

@testset "prod with dims" begin
    B = [1.0 2.0; 3.0 4.0]

    # dims=1: product each column
    P1 = prod(B; dims=1)
    @test P1[1, 1] == 3.0   # 1*3
    @test P1[1, 2] == 8.0   # 2*4

    # dims=2: product each row
    P2 = prod(B; dims=2)
    @test P2[1, 1] == 2.0   # 1*2
    @test P2[2, 1] == 12.0  # 3*4
end

@testset "maximum with dims" begin
    # dims=1: maximum each column
    M1 = maximum(A; dims=1)
    @test M1[1, 1] == 7.0
    @test M1[1, 2] == 8.0
    @test M1[1, 3] == 9.0

    # dims=2: maximum each row
    M2 = maximum(A; dims=2)
    @test M2[1, 1] == 3.0
    @test M2[2, 1] == 6.0
    @test M2[3, 1] == 9.0
end

@testset "minimum with dims" begin
    # dims=1: minimum each column
    M1 = minimum(A; dims=1)
    @test M1[1, 1] == 1.0
    @test M1[1, 2] == 2.0
    @test M1[1, 3] == 3.0

    # dims=2: minimum each row
    M2 = minimum(A; dims=2)
    @test M2[1, 1] == 1.0
    @test M2[2, 1] == 4.0
    @test M2[3, 1] == 7.0
end

@testset "extrema with dims" begin
    # dims=1: extrema each column → array of (min, max) tuples
    E1 = extrema(A; dims=1)
    @test E1[1] == (1.0, 7.0)
    @test E1[2] == (2.0, 8.0)
    @test E1[3] == (3.0, 9.0)

    # dims=2: extrema each row → array of (min, max) tuples
    E2 = extrema(A; dims=2)
    @test E2[1] == (1.0, 3.0)
    @test E2[2] == (4.0, 6.0)
    @test E2[3] == (7.0, 9.0)
end

true
