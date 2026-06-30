using Test

@testset "HOF reduce min/max specialization (Issue #5094)" begin
    xs = [-3, 0, 4, 2]
    @test reduce(min, xs) == -3
    @test typeof(reduce(min, xs)) == Int64
    @test reduce(max, xs) == 4
    @test typeof(reduce(max, xs)) == Int64

    i32s = Int32[-3, 0, 4, 2]
    @test reduce(min, i32s) == Int32(-3)
    @test typeof(reduce(min, i32s)) == Int32
    @test reduce(max, i32s) == Int32(4)
    @test typeof(reduce(max, i32s)) == Int32
    @test reduce(min, Int32[0, 4, 2]; init=Int32(-5)) == Int32(-5)
    @test typeof(reduce(min, Int32[0, 4, 2]; init=Int32(-5))) == Int32
    @test reduce(max, Int32[-3, 0, 2]; init=Int32(9)) == Int32(9)
    @test typeof(reduce(max, Int32[-3, 0, 2]; init=Int32(9))) == Int32

    u32s = UInt32[3, 0, 4, 2]
    @test reduce(min, u32s) == UInt32(0)
    @test typeof(reduce(min, u32s)) == UInt32
    @test reduce(max, u32s) == UInt32(4)
    @test typeof(reduce(max, u32s)) == UInt32

    f32s = Float32[-1.5, 0.0, 2.5]
    @test reduce(min, f32s) == Float32(-1.5)
    @test typeof(reduce(min, f32s)) == Float32
    @test reduce(max, f32s) == Float32(2.5)
    @test typeof(reduce(max, f32s)) == Float32
end

true
