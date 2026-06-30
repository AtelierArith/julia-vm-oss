using Test

i64_global_roundtrip_5081 = 41
i64_global_reassign_5081 = 2

@testset "Int64 fallback locals use locals_any carrier" begin
    global i64_global_roundtrip_5081
    global i64_global_reassign_5081

    @test typeof(i64_global_roundtrip_5081) === Int64
    @test i64_global_roundtrip_5081 == 41
    @test i64_global_reassign_5081 == 2

    i64_global_roundtrip_5081 += 1
    @test typeof(i64_global_roundtrip_5081) === Int64
    @test i64_global_roundtrip_5081 == 42

    i64_global_reassign_5081 = 3.5
    @test i64_global_reassign_5081 == 3.5
end

i64_global_roundtrip_5081 == 42 && i64_global_reassign_5081 == 3.5
