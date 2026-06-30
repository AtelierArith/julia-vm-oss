using Test

tuple_global_roundtrip_5081 = (1, "a")
tuple_global_reassign_5081 = (2, 3)

@testset "Tuple fallback locals use locals_any carrier" begin
    global tuple_global_roundtrip_5081
    global tuple_global_reassign_5081

    @test tuple_global_roundtrip_5081[1] == 1
    @test tuple_global_roundtrip_5081[2] == "a"
    @test length(tuple_global_reassign_5081) == 2

    tuple_global_roundtrip_5081 = (4, 5, 6)
    @test tuple_global_roundtrip_5081[3] == 6
    @test length(tuple_global_roundtrip_5081) == 3

    tuple_global_reassign_5081 = 42
    @test tuple_global_reassign_5081 == 42
end

tuple_global_roundtrip_5081[3] == 6 && tuple_global_reassign_5081 == 42
