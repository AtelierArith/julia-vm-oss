using Test

# Test combine_eltypes and similar for Broadcasted (Issue #2542)

@testset "combine_eltypes" begin
    # Int64 + Int64 → Int64
    @test combine_eltypes(+, ([1, 2, 3], [4, 5, 6])) == Int64

    # Float64 + Float64 → Float64
    @test combine_eltypes(+, ([1.0, 2.0], [3.0, 4.0])) == Float64
end

@testset "similar for Broadcasted" begin
    # 1D Broadcasted: similar creates a Vector
    bc = Broadcasted(nothing, +, ([1, 2, 3], [4, 5, 6]), (1:3,))
    dest = similar(bc, Int64)
    @test length(dest) == 3
end

true
