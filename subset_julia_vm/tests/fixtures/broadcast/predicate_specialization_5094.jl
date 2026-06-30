using Test

@testset "Broadcast predicate specialization (Issue #5094)" begin
    xs = [-3, 0, 4]
    @test broadcast(iszero, xs) == [false, true, false]
    @test typeof(broadcast(iszero, xs)) == BitVector
    @test eltype(broadcast(iszero, xs)) == Bool
    @test broadcast(isone, [0, 1, 2]) == [false, true, false]
    @test typeof(broadcast(isone, [0, 1, 2])) == BitVector
    @test eltype(broadcast(isone, [0, 1, 2])) == Bool
    @test broadcast(signbit, xs) == [true, false, false]
    @test typeof(broadcast(signbit, xs)) == BitVector
    @test eltype(broadcast(signbit, xs)) == Bool

    i32s = Int32[-3, 0, 4]
    @test broadcast(iszero, i32s) == [false, true, false]
    @test eltype(broadcast(iszero, i32s)) == Bool
    @test broadcast(isone, Int32[0, 1, 2]) == [false, true, false]
    @test eltype(broadcast(isone, Int32[0, 1, 2])) == Bool
    @test broadcast(signbit, i32s) == [true, false, false]
    @test eltype(broadcast(signbit, i32s)) == Bool
    @test broadcast(iseven, i32s) == [false, true, true]
    @test eltype(broadcast(iseven, i32s)) == Bool
    @test broadcast(isodd, i32s) == [true, false, false]
    @test eltype(broadcast(isodd, i32s)) == Bool

    u32s = UInt32[0, 1, 4]
    @test broadcast(iszero, u32s) == [true, false, false]
    @test eltype(broadcast(iszero, u32s)) == Bool
    @test broadcast(isone, u32s) == [false, true, false]
    @test eltype(broadcast(isone, u32s)) == Bool
    @test broadcast(iseven, u32s) == [true, false, true]
    @test eltype(broadcast(iseven, u32s)) == Bool
    @test broadcast(isodd, u32s) == [false, true, false]
    @test eltype(broadcast(isodd, u32s)) == Bool

    f32s = Float32[-1.5, 0.0, 1.0]
    @test broadcast(iszero, f32s) == [false, true, false]
    @test eltype(broadcast(iszero, f32s)) == Bool
    @test broadcast(isone, f32s) == [false, false, true]
    @test eltype(broadcast(isone, f32s)) == Bool
    @test broadcast(signbit, f32s) == [true, false, false]
    @test eltype(broadcast(signbit, f32s)) == Bool
end

true
