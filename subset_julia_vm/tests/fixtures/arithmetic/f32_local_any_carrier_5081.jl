using Test

f32_global_roundtrip_5081 = Float32(1.5)
f32_global_reassign_5081 = Float32(2.0)

@testset "Float32 fallback locals use locals_any carrier" begin
    global f32_global_roundtrip_5081
    global f32_global_reassign_5081

    @test typeof(f32_global_roundtrip_5081) === Float32
    @test f32_global_roundtrip_5081 == Float32(1.5)
    @test f32_global_reassign_5081 == Float32(2.0)

    f32_global_roundtrip_5081 = Float32(3.5)
    @test typeof(f32_global_roundtrip_5081) === Float32
    @test f32_global_roundtrip_5081 == Float32(3.5)

    f32_global_reassign_5081 = 42
    @test f32_global_reassign_5081 == 42
end

f32_global_roundtrip_5081 == Float32(3.5) && f32_global_reassign_5081 == 42
