using Test

struct ParamBox4673{T}
    x::T
end

@testset "DataType parameters" begin
    vector_params = Vector{Int64}.parameters
    @test vector_params[1] === Int64

    matrix_params = Matrix{Bool}.parameters
    @test matrix_params[1] === Bool

    dict_params = Dict{Symbol, Int64}.parameters
    @test length(dict_params) == 2
    @test dict_params[1] === Symbol
    @test dict_params[2] === Int64

    tuple_params = Tuple{Int64, Float64}.parameters
    @test length(tuple_params) == 2
    @test tuple_params[1] === Int64
    @test tuple_params[2] === Float64

    nested_params = Dict{String, Vector{Int64}}.parameters
    @test length(nested_params) == 2
    @test nested_params[1] === String
    @test nested_params[2] === Vector{Int64}

    @test length(Int64.parameters) == 0
end

@testset "generic DataType getfield parameters (#4673)" begin
    parameter_from_getfield(::Type{T}) where T = getfield(T, :parameters)[1]
    instance_parameter_from_getfield(x::T) where T = getfield(T, :parameters)[1]

    @test parameter_from_getfield(Vector{Int64}) === Int64
    @test parameter_from_getfield(Matrix{String}) === String
    @test parameter_from_getfield(ParamBox4673{Int64}) === Int64
    @test instance_parameter_from_getfield(ParamBox4673("x")) === String
end

true
