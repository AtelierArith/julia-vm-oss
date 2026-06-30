using Test

@testset "Broadcast n-ary plus specialization (Issue #5094)" begin
    i32 = broadcast(+, Int32[1, -2], Int32[10, 20], Int32[100, 200])
    @test i32 == Int32[111, 218]
    @test typeof(i32) == Vector{Int32}

    f32 = broadcast(+, Float32[1.5, -2.0], Float32[10.0, 20.0], Float32[100.0, 200.0])
    @test f32 == Float32[111.5, 218.0]
    @test typeof(f32) == Vector{Float32}

    bool_sum = broadcast(+, [true, false], [false, true], [true, true])
    @test bool_sum == [2, 2]
    @test typeof(bool_sum) == Vector{Int64}

    four = broadcast(+, Int32[1, 2], Int32[10, 20], Int32[100, 200], Int32[1000, 2000])
    @test four == Int32[1111, 2222]
    @test typeof(four) == Vector{Int32}

    five = broadcast(+, Int32[1, 2], Int32[10, 20], Int32[100, 200], Int32[1000, 2000], Int32[10000, 20000])
    @test five == Int32[11111, 22222]
    @test typeof(five) == Vector{Int32}

    five_singleton = broadcast(+, Int32[1, 2], Int32[10], Int32[100, 200], Int32[1000], Int32[10000, 20000])
    @test five_singleton == Int32[11111, 21212]
    @test typeof(five_singleton) == Vector{Int32}

    five_f32 = broadcast(+, Float32[1.5, -2.0], Float32[10.0, 20.0], Float32[100.0, 200.0], Float32[1000.0, 2000.0], Float32[10000.0, 20000.0])
    @test five_f32 == Float32[11111.5, 22218.0]
    @test typeof(five_f32) == Vector{Float32}

    five_bool_sum = broadcast(+, [true, false], [false, true], [true, true], [true, false], [false, true])
    @test five_bool_sum == [3, 3]
    @test typeof(five_bool_sum) == Vector{Int64}

    mul_i32 = broadcast(*, Int32[2, 3], Int32[4, 5], Int32[6, 7])
    @test mul_i32 == Int32[48, 105]
    @test typeof(mul_i32) == Vector{Int32}

    mul_singleton = broadcast(*, Int32[2, 3], Int32[4], Int32[6, 7])
    @test mul_singleton == Int32[48, 84]
    @test typeof(mul_singleton) == Vector{Int32}

    mul_f32 = broadcast(*, Float32[1.5, -2.0], Float32[4.0, 5.0], Float32[6.0, 7.0])
    @test mul_f32 == Float32[36.0, -70.0]
    @test typeof(mul_f32) == Vector{Float32}

    mul_bool = broadcast(*, [true, false], [true, true], [false, true])
    @test mul_bool == Bool[false, false]
    @test length(mul_bool) == 2

    max_i32 = broadcast(max, Int32[1, 20], Int32[10, -2], Int32[100, 2])
    @test max_i32 == Int32[100, 20]
    @test typeof(max_i32) == Vector{Int32}

    max_singleton = broadcast(max, Int32[1, 20], Int32[10], Int32[100, 2])
    @test max_singleton == Int32[100, 20]
    @test typeof(max_singleton) == Vector{Int32}

    min_f32 = broadcast(min, Float32[1.5, -2.0], Float32[10.0, -20.0], Float32[100.0, 2.0])
    @test min_f32 == Float32[1.5, -20.0]
    @test typeof(min_f32) == Vector{Float32}

end

true
