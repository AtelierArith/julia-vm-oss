using Test

@testset "Broadcast unary specialization (Issue #5094)" begin
    xs = [-3, 0, 4]
    @test broadcast(identity, xs) == xs
    @test typeof(broadcast(identity, xs)) == Vector{Int64}
    @test broadcast(abs, xs) == [3, 0, 4]
    @test typeof(broadcast(abs, xs)) == Vector{Int64}
    @test broadcast(abs2, xs) == [9, 0, 16]
    @test typeof(broadcast(abs2, xs)) == Vector{Int64}
    @test broadcast(-, xs) == [3, 0, -4]
    @test typeof(broadcast(-, xs)) == Vector{Int64}

    i32s = Int32[-3, 0, 4]
    @test broadcast(identity, i32s) == i32s
    @test typeof(broadcast(identity, i32s)) == Vector{Int32}
    @test broadcast(abs, i32s) == Int32[3, 0, 4]
    @test typeof(broadcast(abs, i32s)) == Vector{Int32}
    @test broadcast(abs2, i32s) == Int32[9, 0, 16]
    @test typeof(broadcast(abs2, i32s)) == Vector{Int32}
    @test broadcast(-, i32s) == Int32[3, 0, -4]
    @test typeof(broadcast(-, i32s)) == Vector{Int32}

    f32s = Float32[-1.5, 0.0, 2.0]
    @test broadcast(identity, f32s) == f32s
    @test typeof(broadcast(identity, f32s)) == Vector{Float32}
    @test broadcast(abs, f32s) == Float32[1.5, 0.0, 2.0]
    @test typeof(broadcast(abs, f32s)) == Vector{Float32}
    @test broadcast(abs2, f32s) == Float32[2.25, 0.0, 4.0]
    @test typeof(broadcast(abs2, f32s)) == Vector{Float32}
    @test broadcast(-, f32s) == Float32[1.5, -0.0, -2.0]
    @test typeof(broadcast(-, f32s)) == Vector{Float32}

    bs = [true, false]
    @test broadcast(identity, bs) == bs
    @test typeof(broadcast(identity, bs)) == BitVector
    @test eltype(broadcast(identity, bs)) == Bool
    @test broadcast(abs, bs) == [true, false]
    @test typeof(broadcast(abs, bs)) == BitVector
    @test eltype(broadcast(abs, bs)) == Bool
    @test broadcast(abs2, bs) == [true, false]
    @test typeof(broadcast(abs2, bs)) == BitVector
    @test eltype(broadcast(abs2, bs)) == Bool
end

true
