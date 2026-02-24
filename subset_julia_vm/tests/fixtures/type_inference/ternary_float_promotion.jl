# Test: Ternary/ifelse type inference with Float32/Float16 promotion
# Ensures that conditional branches with mixed float types are correctly promoted
# Regression test for Issue #1892

using Test

function ternary_f32_i64(flag)
    flag ? Float32(1.5) : 0
end

function ternary_f64_f32(flag)
    flag ? 1.5 : Float32(1.0)
end

function ternary_same_f32(flag)
    flag ? Float32(1.5) : Float32(2.5)
end

@testset "Ternary Float32 promotion" begin
    @test ternary_f32_i64(true) == Float32(1.5)
    @test ternary_f32_i64(false) == 0

    @test ternary_f64_f32(true) == 1.5
    @test ternary_f64_f32(false) == 1.0

    @test ternary_same_f32(true) == Float32(1.5)
    @test ternary_same_f32(false) == Float32(2.5)
end

@testset "ifelse Float32 promotion" begin
    @test ifelse(true, Float32(1.5), 0) == Float32(1.5)
    @test ifelse(false, Float32(1.5), 0) == 0
    @test ifelse(true, 1.5, Float32(1.0)) == 1.5
    @test ifelse(false, 1.5, Float32(1.0)) == 1.0
end

true
