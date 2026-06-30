using Test

dict_global_roundtrip_5081 = Dict("a" => 1)
dict_global_reassign_5081 = Dict("b" => 2)

@testset "Dict fallback locals use locals_any carrier" begin
    global dict_global_roundtrip_5081
    global dict_global_reassign_5081

    @test dict_global_roundtrip_5081["a"] == 1
    @test haskey(dict_global_reassign_5081, "b")

    dict_global_roundtrip_5081["c"] = 3
    @test dict_global_roundtrip_5081["c"] == 3

    dict_global_roundtrip_5081 = Dict("x" => 4, "y" => 5)
    @test dict_global_roundtrip_5081["y"] == 5
    @test length(dict_global_roundtrip_5081) == 2

    dict_global_reassign_5081 = 42
    @test dict_global_reassign_5081 == 42
end

dict_global_roundtrip_5081["y"] == 5 && dict_global_reassign_5081 == 42
