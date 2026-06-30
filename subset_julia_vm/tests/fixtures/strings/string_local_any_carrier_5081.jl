using Test

string_global_roundtrip_5081 = "alpha"
string_global_reassign_5081 = "beta"

@testset "String fallback locals use locals_any carrier" begin
    global string_global_roundtrip_5081
    global string_global_reassign_5081

    @test typeof(string_global_roundtrip_5081) === String
    @test string_global_roundtrip_5081 == "alpha"
    @test string_global_reassign_5081 == "beta"

    string_global_roundtrip_5081 = string_global_roundtrip_5081 * "-omega"
    @test typeof(string_global_roundtrip_5081) === String
    @test string_global_roundtrip_5081 == "alpha-omega"

    string_global_reassign_5081 = 42
    @test string_global_reassign_5081 == 42
end

string_global_roundtrip_5081 == "alpha-omega" && string_global_reassign_5081 == 42
