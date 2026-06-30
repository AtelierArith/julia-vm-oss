using Test

@testset "N-ary map specialization (Issue #5094)" begin
    i32 = map(+, Int32[1, -2], Int32[10, 20], Int32[100, 200])
    @test i32 == Int32[111, 218]
    @test typeof(i32) == Vector{Int32}

    f32 = map(+, Float32[1.5, -2.0], Float32[10.0, 20.0], Float32[100.0, 200.0])
    @test f32 == Float32[111.5, 218.0]
    @test typeof(f32) == Vector{Float32}

    bool_sum = map(+, [true, false], [false, true], [true, true])
    @test bool_sum == [2, 2]
    @test typeof(bool_sum) == Vector{Int64}

    four = map(+, Int32[1, 2], Int32[10, 20], Int32[100, 200], Int32[1000, 2000])
    @test four == Int32[1111, 2222]
    @test typeof(four) == Vector{Int32}

    mul_i32 = map(*, Int32[2, 3], Int32[4, 5], Int32[6, 7])
    @test mul_i32 == Int32[48, 105]
    @test typeof(mul_i32) == Vector{Int32}

    mul_f32 = map(*, Float32[1.5, -2.0], Float32[4.0, 5.0], Float32[6.0, 7.0])
    @test mul_f32 == Float32[36.0, -70.0]
    @test typeof(mul_f32) == Vector{Float32}

    mul_bool = map(*, [true, false], [true, true], [false, true])
    @test mul_bool == Bool[false, false]
    @test typeof(mul_bool) == Vector{Bool}

    max_i32 = map(max, Int32[1, 20], Int32[10, -2], Int32[100, 2])
    @test max_i32 == Int32[100, 20]
    @test typeof(max_i32) == Vector{Int32}

    min_f32 = map(min, Float32[1.5, -2.0], Float32[10.0, -20.0], Float32[100.0, 2.0])
    @test min_f32 == Float32[1.5, -20.0]
    @test typeof(min_f32) == Vector{Float32}

    max_bool = map(max, [true, false], [false, false], [true, true])
    @test max_bool == Bool[true, true]
    @test typeof(max_bool) == Vector{Bool}
end

true
