using Test

function typed_vector_from_runtime_type(T)
    T[1, 2]
end

function typed_vector_from_runtime_getindex(T)
    getindex(T, 3, 4)
end

@testset "runtime DataType typed array constructor (#4606)" begin
    int16_values = typed_vector_from_runtime_type(Int16)
    @test typeof(int16_values) === Vector{Int16}
    @test int16_values == Int16[1, 2]

    float32_values = typed_vector_from_runtime_type(Float32)
    @test typeof(float32_values) === Vector{Float32}
    @test length(float32_values) == 2
    @test float32_values[1] === Float32(1)
    @test float32_values[2] === Float32(2)

    getindex_values = typed_vector_from_runtime_getindex(UInt8)
    @test typeof(getindex_values) === Vector{UInt8}
    @test length(getindex_values) == 2
    @test getindex_values[1] === UInt8(3)
    @test getindex_values[2] === UInt8(4)

    real_values = Real[1, 1.5, Float32(2.5)]
    @test typeof(real_values) === Vector{Real}
    @test eltype(real_values) === Real
    @test real_values[1] == 1
    @test real_values[2] === 1.5
    @test real_values[3] === Float32(2.5)

    number_values = Number[1, 1.5, 1 + 2im]
    @test typeof(number_values) === Vector{Number}
    @test eltype(number_values) === Number
    @test number_values[1] == 1
    @test number_values[2] === 1.5
    @test number_values[3] == 1 + 2im
end

true
