using Test

function kwargs_splat_eltype_4282(; kwargs...)
    return eltype(kwargs)
end

function kwargs_splat_collect_type_4282(; kwargs...)
    return typeof(collect(kwargs))
end

function kwargs_splat_typeof_string_4282(; kwargs...)
    return string(typeof(kwargs))
end

@testset "kwargs... Pairs preserve element type and collect (Issue #4282)" begin
    @test string(kwargs_splat_eltype_4282(y=2)) == "Pair{Symbol, Int64}"
    @test string(kwargs_splat_collect_type_4282(y=2, z=3)) == "Vector{Pair{Symbol, Int64}}"
    @test string(kwargs_splat_eltype_4282(y=2, z=3.0)) == "Pair{Symbol, Real}"
    @test string(kwargs_splat_collect_type_4282(y=2, z=3.0)) == "Vector{Pair{Symbol, Real}}"
    @test string(kwargs_splat_eltype_4282(a=1, b="x")) == "Pair{Symbol, Any}"
    @test string(kwargs_splat_collect_type_4282(a=1, b="x")) == "Vector{Pair{Symbol, Any}}"
    @test string(kwargs_splat_eltype_4282()) == "Pair{Symbol, Union{}}"
    @test string(kwargs_splat_collect_type_4282()) == "Vector{Pair{Symbol, Union{}}}"
    @test kwargs_splat_typeof_string_4282(y=2) == "Base.Pairs{Symbol, Int64, Nothing, @NamedTuple{y::Int64}}"
    @test kwargs_splat_typeof_string_4282() == "Base.Pairs{Symbol, Union{}, Nothing, @NamedTuple{}}"

    mixed_type = kwargs_splat_typeof_string_4282(y=2, z=3.0)
    @test startswith(mixed_type, "Base.Pairs{Symbol, Real, Nothing, @NamedTuple{")
    @test occursin("y::Int64", mixed_type)
    @test occursin("z::Float64", mixed_type)
end

true
