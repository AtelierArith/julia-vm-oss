# Test rotl90, rotr90, rot180 matrix rotation functions (Issue #1879)

using Test

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

true
