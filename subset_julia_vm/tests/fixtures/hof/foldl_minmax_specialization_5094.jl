using Test

@testset "HOF foldl min/max specialization (Issue #5094)" begin
    xs = [-3, 0, 4, 2]
    @test foldl(min, xs) == -3
    @test typeof(foldl(min, xs)) == Int64
    @test foldl(max, xs) == 4
    @test typeof(foldl(max, xs)) == Int64

    i32s = Int32[-3, 0, 4, 2]
    @test foldl(min, i32s) == Int32(-3)
    @test typeof(foldl(min, i32s)) == Int32
    @test foldl(max, i32s) == Int32(4)
    @test typeof(foldl(max, i32s)) == Int32
    @test foldl(min, Int32[0, 4, 2]; init=Int32(-5)) == Int32(-5)
    @test typeof(foldl(min, Int32[0, 4, 2]; init=Int32(-5))) == Int32
    @test foldl(max, Int32[-3, 0, 2]; init=Int32(9)) == Int32(9)
    @test typeof(foldl(max, Int32[-3, 0, 2]; init=Int32(9))) == Int32

    u32s = UInt32[3, 0, 4, 2]
    @test foldl(min, u32s) == UInt32(0)
    @test typeof(foldl(min, u32s)) == UInt32
    @test foldl(max, u32s) == UInt32(4)
    @test typeof(foldl(max, u32s)) == UInt32

    f32s = Float32[-1.5, 0.0, 2.5]
    @test foldl(min, f32s) == Float32(-1.5)
    @test typeof(foldl(min, f32s)) == Float32
    @test foldl(max, f32s) == Float32(2.5)
    @test typeof(foldl(max, f32s)) == Float32
end

true
