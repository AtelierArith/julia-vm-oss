using Test

@testset "collect routes public materialization to Array wrapper (#6649)" begin
    range_values = collect(1:3)
    @test typeof(range_values) === Vector{Int64}
    @test typeof(range_values.ref) == MemoryRef{Int64}
    @test range_values.size == (3,)
    @test range_values == [1, 2, 3]

    stepped = collect(1:2:7)
    @test typeof(stepped) === Vector{Int64}
    @test typeof(stepped.ref) == MemoryRef{Int64}
    @test stepped == [1, 3, 5, 7]

    floats = collect(1.0:0.5:2.0)
    @test typeof(floats) === Vector{Float64}
    @test typeof(floats.ref) == MemoryRef{Float64}
    @test floats == [1.0, 1.5, 2.0]

    tuple_values = collect((1, 2.5))
    @test typeof(tuple_values) === Vector{Real}
    @test typeof(tuple_values.ref) == MemoryRef{Real}
    @test eltype(tuple_values) == Real
    @test tuple_values[1] == 1
    @test tuple_values[2] == 2.5

    source = Int16[1, 2]
    copied = collect(source)
    @test typeof(copied) === Vector{Int16}
    @test typeof(copied.ref) == MemoryRef{Int16}
    copied[1] = Int16(9)
    @test source[1] == Int16(1)
    @test copied == Int16[9, 2]
end

true
