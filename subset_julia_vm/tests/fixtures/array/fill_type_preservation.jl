using Test

@testset "fill preserves value type" begin
    @test convert(Symbol, :x) == :x

    f32s = fill(Float32(1.5), 3)
    @test eltype(f32s) == Float32
    @test length(f32s) == 3
    @test f32s[1] == Float32(1.5)
    @test f32s[3] == Float32(1.5)

    ints = fill(7, (2, 2))
    @test eltype(ints) == Int64
    @test size(ints) == (2, 2)
    @test ints[2, 2] == 7

    syms = fill(:x, 3)
    @test typeof(syms) == Vector{Symbol}
    @test eltype(syms) == Symbol
    @test syms[1] == :x
    @test syms[3] == :x

    symmat = fill(:y, (2, 2))
    @test typeof(symmat) == Matrix{Symbol}
    @test eltype(symmat) == Symbol
    @test size(symmat) == (2, 2)
    @test symmat[2, 2] == :y

    simsyms = similar(Array{Symbol}, 2)
    simsyms[1] = :a
    @test typeof(simsyms) == Vector{Symbol}
    @test eltype(simsyms) == Symbol
    @test simsyms[1] == :a
end

true
