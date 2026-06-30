# CartesianIndex arithmetic and index conversion (Issue #5136)
# Tests the splat constructor, +/-/* arithmetic, and LinearIndices /
# CartesianIndices index conversion (linear <-> cartesian).

using Test

@testset "CartesianIndex splat constructor (Issue #5136)" begin
    a = CartesianIndex(1, 2)
    @test a.I == (1, 2)
    @test length(a) == 2

    b = CartesianIndex(3, 5, 7)
    @test b.I == (3, 5, 7)
    @test length(b) == 3

    # single-tuple constructor still works
    c = CartesianIndex((4, 6))
    @test c.I == (4, 6)
end

@testset "CartesianIndex arithmetic (Issue #5136)" begin
    a = CartesianIndex(1, 2)
    b = CartesianIndex(3, 5)

    @test a + b == CartesianIndex(4, 7)
    @test b - a == CartesianIndex(2, 3)
    @test -a == CartesianIndex(-1, -2)
    @test +a == CartesianIndex(1, 2)

    # scalar multiplication (both orders)
    @test a * 2 == CartesianIndex(2, 4)
    @test 2 * a == CartesianIndex(2, 4)

    # 3D arithmetic
    p = CartesianIndex(1, 2, 3)
    q = CartesianIndex(10, 20, 30)
    @test p + q == CartesianIndex(11, 22, 33)
    @test q - p == CartesianIndex(9, 18, 27)
end

@testset "LinearIndices Cartesian->linear conversion (Issue #5136)" begin
    li = LinearIndices((2, 3))
    @test li[1, 1] == 1
    @test li[2, 1] == 2
    @test li[1, 2] == 3
    @test li[2, 2] == 4
    @test li[1, 3] == 5
    @test li[2, 3] == 6

    # indexing via CartesianIndex
    @test li[CartesianIndex(2, 1)] == 2
    @test li[CartesianIndex(2, 3)] == 6

    # 3D
    li3 = LinearIndices((2, 2, 2))
    @test li3[1, 1, 1] == 1
    @test li3[2, 1, 1] == 2
    @test li3[1, 2, 1] == 3
    @test li3[2, 2, 2] == 8
end

@testset "CartesianIndices linear->Cartesian conversion (Issue #5136)" begin
    ci = CartesianIndices((2, 3))
    @test ci[1] == CartesianIndex(1, 1)
    @test ci[2] == CartesianIndex(2, 1)
    @test ci[3] == CartesianIndex(1, 2)
    @test ci[6] == CartesianIndex(2, 3)
end

true  # Test passed
