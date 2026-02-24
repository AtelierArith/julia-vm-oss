# Float field type preservation test
# Ensures that parametric struct fields preserve their float type (F16, F32, F64)
# Prevention test for Issue #1651 / #1655

using Test

struct FloatHolder{T}
    value::T
end

@testset "Float32 field type preservation" begin
    h = FloatHolder{Float32}(Float32(1.5))
    @test typeof(h.value) === Float32
    @test h.value == Float32(1.5)
end

@testset "Float64 field type preservation" begin
    h = FloatHolder{Float64}(1.5)
    @test typeof(h.value) === Float64
    @test h.value == 1.5
end

@testset "Float16 field type preservation" begin
    h = FloatHolder{Float16}(Float16(1.5))
    @test typeof(h.value) === Float16
    @test h.value == Float16(1.5)
end

true
