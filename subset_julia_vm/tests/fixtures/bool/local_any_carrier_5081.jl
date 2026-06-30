using Test

bool_global_roundtrip_5081 = true
bool_global_reassign_5081 = false

@testset "Bool fallback locals use locals_any carrier" begin
    global bool_global_roundtrip_5081
    global bool_global_reassign_5081

    @test bool_global_roundtrip_5081 === true
    @test bool_global_reassign_5081 === false

    bool_global_roundtrip_5081 = false
    @test bool_global_roundtrip_5081 === false

    bool_global_reassign_5081 = 42
    @test bool_global_reassign_5081 == 42
end

bool_global_roundtrip_5081 === false && bool_global_reassign_5081 == 42
