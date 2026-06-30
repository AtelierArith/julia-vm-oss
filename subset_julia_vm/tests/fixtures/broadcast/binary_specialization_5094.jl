using Test

@testset "Broadcast binary specialization (Issue #5094)" begin
    lhs_i32 = Int32[1, -2, 4]
    rhs_i32 = Int32[5, 6, -7]
    @test broadcast(+, lhs_i32, rhs_i32) == Int32[6, 4, -3]
    @test typeof(broadcast(+, lhs_i32, rhs_i32)) == Vector{Int32}
    @test broadcast(-, lhs_i32, rhs_i32) == Int32[-4, -8, 11]
    @test typeof(broadcast(-, lhs_i32, rhs_i32)) == Vector{Int32}
    @test broadcast(*, lhs_i32, rhs_i32) == Int32[5, -12, -28]
    @test typeof(broadcast(*, lhs_i32, rhs_i32)) == Vector{Int32}
    @test broadcast(min, lhs_i32, rhs_i32) == Int32[1, -2, -7]
    @test typeof(broadcast(min, lhs_i32, rhs_i32)) == Vector{Int32}
    @test broadcast(max, lhs_i32, rhs_i32) == Int32[5, 6, 4]
    @test typeof(broadcast(max, lhs_i32, rhs_i32)) == Vector{Int32}
    @test broadcast(/, lhs_i32, Int32[1, 2, 4]) == [1.0, -1.0, 1.0]
    @test typeof(broadcast(/, lhs_i32, Int32[1, 2, 4])) == Vector{Float64}

    lhs_f32 = Float32[1.5, -2.0, 4.0]
    rhs_f32 = Float32[2.0, 3.5, -0.5]
    @test broadcast(+, lhs_f32, rhs_f32) == Float32[3.5, 1.5, 3.5]
    @test typeof(broadcast(+, lhs_f32, rhs_f32)) == Vector{Float32}
    @test broadcast(-, lhs_f32, rhs_f32) == Float32[-0.5, -5.5, 4.5]
    @test typeof(broadcast(-, lhs_f32, rhs_f32)) == Vector{Float32}
    @test broadcast(*, lhs_f32, rhs_f32) == Float32[3.0, -7.0, -2.0]
    @test typeof(broadcast(*, lhs_f32, rhs_f32)) == Vector{Float32}
    @test broadcast(min, lhs_f32, rhs_f32) == Float32[1.5, -2.0, -0.5]
    @test typeof(broadcast(min, lhs_f32, rhs_f32)) == Vector{Float32}
    @test broadcast(/, lhs_f32, Float32[0.5, 2.0, -4.0]) == Float32[3.0, -1.0, -1.0]
    @test typeof(broadcast(/, lhs_f32, Float32[0.5, 2.0, -4.0])) == Vector{Float32}

    lhs_f64 = [3.0, -2.0, 4.0]
    @test broadcast(/, lhs_f64, [0.5, 2.0, -4.0]) == [6.0, -1.0, -1.0]
    @test typeof(broadcast(/, lhs_f64, [0.5, 2.0, -4.0])) == Vector{Float64}

    lhs_bool = [true, false, true]
    rhs_bool = [false, true, true]
    @test broadcast(+, lhs_bool, rhs_bool) == [1, 1, 2]
    @test typeof(broadcast(+, lhs_bool, rhs_bool)) == Vector{Int64}
    @test broadcast(/, lhs_bool, [true, true, true]) == [1.0, 0.0, 1.0]
    @test typeof(broadcast(/, lhs_bool, [true, true, true])) == Vector{Float64}

    @test broadcast(+, Int32[1, 2, 3], Int32[10]) == Int32[11, 12, 13]
    @test broadcast(/, Int32[2, 4, 8], Int32[2]) == [1.0, 2.0, 4.0]
    @test broadcast(/, Float32[2.0, 4.0], Float32[2.0]) == Float32[1.0, 2.0]
end

true
