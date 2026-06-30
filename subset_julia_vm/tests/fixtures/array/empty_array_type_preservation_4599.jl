using Test

@testset "empty array preserves requested element type (#4018, #4599)" begin
    float_empty = empty(Float32[1, 2])
    @test typeof(float_empty) === Vector{Float32}
    @test eltype(float_empty) === Float32
    @test length(float_empty) == 0

    int8_empty = empty(Int8[1, 2])
    @test typeof(int8_empty) === Vector{Int8}
    @test eltype(int8_empty) === Int8
    @test length(int8_empty) == 0

    requested_empty = empty(Float32[1, 2], Int16)
    @test typeof(requested_empty) === Vector{Int16}
    @test eltype(requested_empty) === Int16
    @test length(requested_empty) == 0

    any_empty = empty(Any[1, "x"])
    @test typeof(any_empty) === Vector{Any}
    @test eltype(any_empty) === Any
    @test length(any_empty) == 0
end

true
