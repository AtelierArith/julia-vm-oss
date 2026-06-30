using Test

@testset "TypeVar and UnionAll reflection" begin
    tv = Vector.var
    @test tv.name === :T
    @test tv.lb === Union{}
    @test tv.ub === Any

    vector_body = Vector.body
    vector_body_params = vector_body.parameters
    @test vector_body_params[1] === tv

    dict_k = Dict.var
    dict_body = Dict.body
    dict_v = dict_body.var
    dict_concrete_body = dict_body.body
    dict_params = dict_concrete_body.parameters

    @test dict_k.name === :K
    @test dict_v.name === :V
    @test dict_params[1] === dict_k
    @test dict_params[2] === dict_v
end

@testset "eltype for reflected type parameters" begin
    @test eltype(Array) === Any
    @test eltype(Vector) === Any
    @test eltype(Vector{Int64}) === Int64
    @test eltype(Matrix{Bool}) === Bool
end

true
