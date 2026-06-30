# Test splice! with range indices (Issue #3481)

using Test

@testset "splice! with range indices" begin
    # splice!(a, r) - remove and return elements in range
    a1 = [1, 2, 3, 4, 5]
    removed1 = splice!(a1, 2:4)
    @test removed1 == [2, 3, 4]
    @test length(a1) == 2
    @test a1[1] == 1
    @test a1[2] == 5

    # splice!(a, r) - remove first element via range
    a2 = [10, 20, 30, 40]
    removed2 = splice!(a2, 1:2)
    @test removed2 == [10, 20]
    @test length(a2) == 2
    @test a2[1] == 30
    @test a2[2] == 40

    # splice!(a, r, ins) - remove range and insert replacement
    a3 = [1, 2, 3, 4, 5]
    removed3 = splice!(a3, 2:3, [20, 30, 40])
    @test removed3 == [2, 3]
    @test length(a3) == 6
    @test a3[1] == 1
    @test a3[2] == 20
    @test a3[3] == 30
    @test a3[4] == 40
    @test a3[5] == 4
    @test a3[6] == 5

    # splice!(a, r, ins) - replace with fewer elements (shrinks)
    a4 = [1, 2, 3, 4, 5]
    removed4 = splice!(a4, 2:4, [99])
    @test removed4 == [2, 3, 4]
    @test length(a4) == 3
    @test a4[1] == 1
    @test a4[2] == 99
    @test a4[3] == 5
end

true
