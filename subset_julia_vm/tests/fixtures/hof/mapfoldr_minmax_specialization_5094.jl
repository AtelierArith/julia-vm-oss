using Test

@testset "HOF mapfoldr identity min/max specialization (Issue #5094)" begin
    xs = [-3, 0, 4, 2]
    @test mapfoldr(identity, min, xs) == -3
    @test typeof(mapfoldr(identity, min, xs)) == Int64
    @test mapfoldr(identity, max, xs) == 4
    @test typeof(mapfoldr(identity, max, xs)) == Int64

    i32s = Int32[-3, 0, 4, 2]
    @test mapfoldr(identity, min, i32s) == Int32(-3)
    @test typeof(mapfoldr(identity, min, i32s)) == Int32
    @test mapfoldr(identity, max, i32s) == Int32(4)
    @test typeof(mapfoldr(identity, max, i32s)) == Int32
    @test mapfoldr(identity, min, Int32[0, 4, 2]; init=Int32(-5)) == Int32(-5)
    @test typeof(mapfoldr(identity, min, Int32[0, 4, 2]; init=Int32(-5))) == Int32
    @test mapfoldr(identity, max, Int32[-3, 0, 2]; init=Int32(9)) == Int32(9)
    @test typeof(mapfoldr(identity, max, Int32[-3, 0, 2]; init=Int32(9))) == Int32

    u32s = UInt32[3, 0, 4, 2]
    @test mapfoldr(identity, min, u32s) == UInt32(0)
    @test typeof(mapfoldr(identity, min, u32s)) == UInt32
    @test mapfoldr(identity, max, u32s) == UInt32(4)
    @test typeof(mapfoldr(identity, max, u32s)) == UInt32

    f32s = Float32[-1.5, 0.0, 2.5]
    @test mapfoldr(identity, min, f32s) == Float32(-1.5)
    @test typeof(mapfoldr(identity, min, f32s)) == Float32
    @test mapfoldr(identity, max, f32s) == Float32(2.5)
    @test typeof(mapfoldr(identity, max, f32s)) == Float32
end

true
