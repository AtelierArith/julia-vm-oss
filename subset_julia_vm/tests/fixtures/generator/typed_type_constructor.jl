using Test

runtime_collect(x) = collect(x)

@testset "Typed Base.Generator constructor" begin
    g = Base.Generator(Float64, [1, 2, 3])
    @test typeof(g) === Base.Generator{Vector{Int64}, Type{Float64}}
    @test typeof(Base.IteratorEltype(g)) === typeof(Base.EltypeUnknown())
    @test typeof(Base.IteratorEltype(typeof(g))) === typeof(Base.EltypeUnknown())

    values = collect(g)
    @test typeof(values) === Vector{Float64}
    @test eltype(values) === Float64
    @test values == [1.0, 2.0, 3.0]

    iter_g = Base.Generator(Float64, [1, 2])
    first_step = iterate(iter_g)
    @test first_step[1] === 1.0
    @test typeof(first_step[1]) === Float64
    second_step = iterate(iter_g, first_step[2])
    @test second_step[1] === 2.0
    @test typeof(second_step[1]) === Float64
    @test iterate(iter_g, second_step[2]) === nothing

    runtime_values = runtime_collect(Base.Generator(Float64, [1, 2, 3]))
    @test typeof(runtime_values) === Vector{Float64}
    @test eltype(runtime_values) === Float64
    @test runtime_values == [1.0, 2.0, 3.0]

    empty_values = collect(Base.Generator(Float64, Int64[]))
    @test typeof(empty_values) === Vector{Float64}
    @test eltype(empty_values) === Float64
    @test length(empty_values) == 0

    empty_range_values = collect(Base.Generator(Float64, 5:4))
    @test typeof(empty_range_values) === Vector{Float64}
    @test eltype(empty_range_values) === Float64
    @test length(empty_range_values) == 0

    int8_values = collect(Base.Generator(Int8, 1:3))
    @test typeof(int8_values) === Vector{Int8}
    @test eltype(int8_values) === Int8
    @test int8_values[1] == Int8(1)
    @test int8_values[2] == Int8(2)
    @test int8_values[3] == Int8(3)
end

true
