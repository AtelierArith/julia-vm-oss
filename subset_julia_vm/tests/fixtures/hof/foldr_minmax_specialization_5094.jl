using Test

@testset "HOF foldr min/max specialization (Issue #5094)" begin
    xs = [-3, 0, 4, 2]
    @test foldr(min, xs) == -3
    @test typeof(foldr(min, xs)) == Int64
    @test foldr(max, xs) == 4
    @test typeof(foldr(max, xs)) == Int64

    i32s = Int32[-3, 0, 4, 2]
    @test foldr(min, i32s) == Int32(-3)
    @test typeof(foldr(min, i32s)) == Int32
    @test foldr(max, i32s) == Int32(4)
    @test typeof(foldr(max, i32s)) == Int32
    @test foldr(min, Int32[0, 4, 2]; init=Int32(-5)) == Int32(-5)
    @test typeof(foldr(min, Int32[0, 4, 2]; init=Int32(-5))) == Int32
    @test foldr(max, Int32[-3, 0, 2]; init=Int32(9)) == Int32(9)
    @test typeof(foldr(max, Int32[-3, 0, 2]; init=Int32(9))) == Int32

    u32s = UInt32[3, 0, 4, 2]
    @test foldr(min, u32s) == UInt32(0)
    @test typeof(foldr(min, u32s)) == UInt32
    @test foldr(max, u32s) == UInt32(4)
    @test typeof(foldr(max, u32s)) == UInt32

    f32s = Float32[-1.5, 0.0, 2.5]
    @test foldr(min, f32s) == Float32(-1.5)
    @test typeof(foldr(min, f32s)) == Float32
    @test foldr(max, f32s) == Float32(2.5)
    @test typeof(foldr(max, f32s)) == Float32
end

true
