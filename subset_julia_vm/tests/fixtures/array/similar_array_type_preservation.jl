using Test

@testset "similar(Array{T}, dims...) preserves element type" begin
    ints = similar(Array{Int64}, 2)
    @test typeof(ints) == Vector{Int64}
    @test eltype(ints) == Int64
    @test length(ints) == 2

    f32s = similar(Array{Float32}, (2,))
    @test typeof(f32s) == Vector{Float32}
    @test eltype(f32s) == Float32
    @test length(f32s) == 2

    syms = similar(Array{Symbol}, 2)
    @test typeof(syms) == Vector{Symbol}
    @test eltype(syms) == Symbol
    @test length(syms) == 2

    mem = Memory{Int64}(2)
    wrapped = wrap(Array, mem, (2,))
    @test eltype(wrapped) == Int64
end

true
