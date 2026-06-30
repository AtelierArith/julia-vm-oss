using Test

range_global_roundtrip_5081 = 1:4
range_global_reassign_5081 = 2:5

@testset "Range fallback locals use locals_any carrier" begin
    global range_global_roundtrip_5081
    global range_global_reassign_5081

    @test collect(range_global_roundtrip_5081) == [1, 2, 3, 4]
    @test length(range_global_roundtrip_5081) == 4
    @test first(range_global_reassign_5081) == 2

    range_global_roundtrip_5081 = 3:6
    @test collect(range_global_roundtrip_5081) == [3, 4, 5, 6]
    @test last(range_global_roundtrip_5081) == 6

    range_global_reassign_5081 = 42
    @test range_global_reassign_5081 == 42
end

collect(range_global_roundtrip_5081) == [3, 4, 5, 6] && range_global_reassign_5081 == 42
