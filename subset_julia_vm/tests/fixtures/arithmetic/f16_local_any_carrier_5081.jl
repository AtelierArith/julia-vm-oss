using Test

f16_global_roundtrip_5081 = Float16(1.5)
f16_global_reassign_5081 = Float16(2.0)

@testset "Float16 fallback locals use locals_any carrier" begin
    global f16_global_roundtrip_5081
    global f16_global_reassign_5081

    @test typeof(f16_global_roundtrip_5081) === Float16
    @test f16_global_roundtrip_5081 == Float16(1.5)
    @test f16_global_reassign_5081 == Float16(2.0)

    f16_global_roundtrip_5081 = Float16(3.5)
    @test typeof(f16_global_roundtrip_5081) === Float16
    @test f16_global_roundtrip_5081 == Float16(3.5)

    f16_global_reassign_5081 = 42
    @test f16_global_reassign_5081 == 42
end

f16_global_roundtrip_5081 == Float16(3.5) && f16_global_reassign_5081 == 42
