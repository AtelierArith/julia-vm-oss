using Test

@testset "Array{Any}/Array{Real} preserve boxed numeric values (#4646)" begin
    any_values = Array{Any}(undef, 8)
    any_values[1] = Int8(1)
    any_values[2] = Int16(2)
    any_values[3] = Int32(3)
    any_values[4] = Int64(4)
    any_values[5] = UInt8(5)
    any_values[6] = UInt16(6)
    any_values[7] = UInt32(7)
    any_values[8] = Float32(8)

    @test typeof(any_values) == Vector{Any}
    @test eltype(any_values) == Any
    @test typeof(any_values[1]) == Int8
    @test typeof(any_values[2]) == Int16
    @test typeof(any_values[3]) == Int32
    @test typeof(any_values[4]) == Int64
    @test typeof(any_values[5]) == UInt8
    @test typeof(any_values[6]) == UInt16
    @test typeof(any_values[7]) == UInt32
    @test typeof(any_values[8]) == Float32

    real_values = Array{Real}(undef, 2)
    real_values[1] = Int8(1)
    real_values[2] = Float32(2)

    @test typeof(real_values) == Vector{Real}
    @test eltype(real_values) == Real
    @test typeof(real_values[1]) == Int8
    @test typeof(real_values[2]) == Float32

    float_values = Vector{Float64}(undef, 1)
    float_values[1] = Int32(3)
    @test typeof(float_values[1]) == Float64
    @test float_values[1] == 3.0
end

true
