using Test

@testset "typed matrix literal lowering (#4575, #4629)" begin
    f32 = Float32[1 2; 3 4]
    @test typeof(f32) === Matrix{Float32}
    @test eltype(f32) === Float32
    @test size(f32) == (2, 2)
    @test typeof(f32[1, 1]) === Float32
    @test f32[1, 2] == Float32(2)
    @test f32[2, 1] == Float32(3)

    real_values = Real[1 2.5; 3 Float32(4.5)]
    @test typeof(real_values) === Matrix{Real}
    @test eltype(real_values) === Real
    @test size(real_values) == (2, 2)
    @test typeof(real_values[1, 1]) === Int64
    @test typeof(real_values[1, 2]) === Float64
    @test real_values[1, 2] === 2.5
    @test typeof(real_values[2, 2]) === Float32
    @test real_values[2, 2] === Float32(4.5)
end

true
