using Test

f64_global_roundtrip_5081 = 1.5
f64_global_reassign_5081 = 2.0

@testset "Float64 fallback locals use locals_any carrier" begin
    global f64_global_roundtrip_5081
    global f64_global_reassign_5081

    @test typeof(f64_global_roundtrip_5081) === Float64
    @test f64_global_roundtrip_5081 == 1.5
    @test f64_global_reassign_5081 == 2.0

    f64_global_roundtrip_5081 = 3.5
    @test typeof(f64_global_roundtrip_5081) === Float64
    @test f64_global_roundtrip_5081 == 3.5

    f64_global_reassign_5081 = 42
    @test f64_global_reassign_5081 == 42
end

f64_global_roundtrip_5081 == 3.5 && f64_global_reassign_5081 == 42
