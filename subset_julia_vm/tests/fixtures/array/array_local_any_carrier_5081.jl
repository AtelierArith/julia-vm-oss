using Test

array_global_roundtrip_5081 = [1, 2, 3]
array_global_reassign_5081 = [4, 5]

@testset "Array fallback locals use locals_any carrier" begin
    global array_global_roundtrip_5081
    global array_global_reassign_5081

    @test array_global_roundtrip_5081[1] == 1
    @test length(array_global_reassign_5081) == 2

    push!(array_global_roundtrip_5081, 4)
    @test array_global_roundtrip_5081[4] == 4
    @test length(array_global_roundtrip_5081) == 4

    array_global_roundtrip_5081 = [6, 7, 8]
    @test array_global_roundtrip_5081[3] == 8

    array_global_reassign_5081 = 42
    @test array_global_reassign_5081 == 42
end

array_global_roundtrip_5081[3] == 8 && array_global_reassign_5081 == 42
