using Test

@testset "Binary map specialization (Issue #5094)" begin
    lhs_i32 = Int32[1, -2, 4]
    rhs_i32 = Int32[5, 6, -7]
    @test map(+, lhs_i32, rhs_i32) == Int32[6, 4, -3]
    @test typeof(map(+, lhs_i32, rhs_i32)) == Vector{Int32}
    @test map(-, lhs_i32, rhs_i32) == Int32[-4, -8, 11]
    @test typeof(map(-, lhs_i32, rhs_i32)) == Vector{Int32}
    @test map(*, lhs_i32, rhs_i32) == Int32[5, -12, -28]
    @test typeof(map(*, lhs_i32, rhs_i32)) == Vector{Int32}
    @test map(min, lhs_i32, rhs_i32) == Int32[1, -2, -7]
    @test typeof(map(min, lhs_i32, rhs_i32)) == Vector{Int32}
    @test map(max, lhs_i32, rhs_i32) == Int32[5, 6, 4]
    @test typeof(map(max, lhs_i32, rhs_i32)) == Vector{Int32}
    @test map(/, lhs_i32, Int32[1, 2, 4]) == [1.0, -1.0, 1.0]
    @test typeof(map(/, lhs_i32, Int32[1, 2, 4])) == Vector{Float64}

    lhs_f32 = Float32[1.5, -2.0, 4.0]
    @test map(min, lhs_f32, Float32[0.5, 2.0, -4.0]) == Float32[0.5, -2.0, -4.0]
    @test typeof(map(min, lhs_f32, Float32[0.5, 2.0, -4.0])) == Vector{Float32}
    @test map(/, lhs_f32, Float32[0.5, 2.0, -4.0]) == Float32[3.0, -1.0, -1.0]
    @test typeof(map(/, lhs_f32, Float32[0.5, 2.0, -4.0])) == Vector{Float32}

    lhs_bool = [true, false, true]
    @test map(+, lhs_bool, [false, true, true]) == [1, 1, 2]
    @test typeof(map(+, lhs_bool, [false, true, true])) == Vector{Int64}
    @test map(max, lhs_bool, [false, true, true]) == [true, true, true]
    @test typeof(map(max, lhs_bool, [false, true, true])) == Vector{Bool}
    @test map(/, lhs_bool, [true, true, true]) == [1.0, 0.0, 1.0]
    @test typeof(map(/, lhs_bool, [true, true, true])) == Vector{Float64}
end

true
