using Test

map_empty_double_4153(x) = x * 2
map_empty_to_float_4153(x) = x + 0.5
map_empty_collect_generator_4153(f, A) = collect(Base.Generator(f, A))

@testset "map empty default eltype (Issue #4153)" begin
    ints = map(Int64, Int64[])
    @test typeof(ints) === Vector{Int64}
    @test eltype(ints) === Int64
    @test length(ints) == 0

    floats = map(Float64, Int64[])
    @test typeof(floats) === Vector{Float64}
    @test eltype(floats) === Float64
    @test length(floats) == 0

    doubled = map(map_empty_double_4153, Int64[])
    @test typeof(doubled) === Vector{Int64}
    @test eltype(doubled) === Int64
    @test length(doubled) == 0

    widened = map(map_empty_to_float_4153, Int64[])
    @test typeof(widened) === Vector{Float64}
    @test eltype(widened) === Float64
    @test length(widened) == 0

    runtime_doubled = map_empty_collect_generator_4153(map_empty_double_4153, Int64[])
    @test typeof(runtime_doubled) === Vector{Int64}
    @test eltype(runtime_doubled) === Int64
    @test length(runtime_doubled) == 0

    runtime_widened = map_empty_collect_generator_4153(map_empty_to_float_4153, Int64[])
    @test typeof(runtime_widened) === Vector{Float64}
    @test eltype(runtime_widened) === Float64
    @test length(runtime_widened) == 0
end

true
