using Test

char_global_roundtrip_5081 = 'x'
char_global_reassign_5081 = 'y'

@testset "Char fallback locals use locals_any carrier" begin
    global char_global_roundtrip_5081
    global char_global_reassign_5081

    @test char_global_roundtrip_5081 == 'x'
    @test char_global_reassign_5081 == 'y'

    char_global_roundtrip_5081 = 'z'
    @test char_global_roundtrip_5081 == 'z'

    char_global_reassign_5081 = 42
    @test char_global_reassign_5081 == 42
end

char_global_roundtrip_5081 == 'z' && char_global_reassign_5081 == 42
