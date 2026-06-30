using Test

@testset "HOF mapfoldl identity min/max specialization (Issue #5094)" begin
    xs = [-3, 0, 4, 2]
    @test mapfoldl(identity, min, xs) == -3
    @test typeof(mapfoldl(identity, min, xs)) == Int64
    @test mapfoldl(identity, max, xs) == 4
    @test typeof(mapfoldl(identity, max, xs)) == Int64

    i32s = Int32[-3, 0, 4, 2]
    @test mapfoldl(identity, min, i32s) == Int32(-3)
    @test typeof(mapfoldl(identity, min, i32s)) == Int32
    @test mapfoldl(identity, max, i32s) == Int32(4)
    @test typeof(mapfoldl(identity, max, i32s)) == Int32
    @test mapfoldl(identity, min, Int32[0, 4, 2]; init=Int32(-5)) == Int32(-5)
    @test typeof(mapfoldl(identity, min, Int32[0, 4, 2]; init=Int32(-5))) == Int32
    @test mapfoldl(identity, max, Int32[-3, 0, 2]; init=Int32(9)) == Int32(9)
    @test typeof(mapfoldl(identity, max, Int32[-3, 0, 2]; init=Int32(9))) == Int32

    u32s = UInt32[3, 0, 4, 2]
    @test mapfoldl(identity, min, u32s) == UInt32(0)
    @test typeof(mapfoldl(identity, min, u32s)) == UInt32
    @test mapfoldl(identity, max, u32s) == UInt32(4)
    @test typeof(mapfoldl(identity, max, u32s)) == UInt32

    f32s = Float32[-1.5, 0.0, 2.5]
    @test mapfoldl(identity, min, f32s) == Float32(-1.5)
    @test typeof(mapfoldl(identity, min, f32s)) == Float32
    @test mapfoldl(identity, max, f32s) == Float32(2.5)
    @test typeof(mapfoldl(identity, max, f32s)) == Float32
end

true
