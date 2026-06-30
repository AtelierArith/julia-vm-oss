using Test

@testset "runtime type object identity projections" begin
    @test objectid(Int64) == objectid(Int64)
    @test objectid(Int64) != objectid(Float64)
    @test objectid(Vector) == objectid(Vector)
    @test objectid(Vector{Int64}) != objectid(Vector{Float64})

    vector_t = Vector.var
    @test vector_t === Vector.body.parameters[1]

    dict_k = Dict.var
    dict_v = Dict.body.var
    dict_params = Dict.body.body.parameters
    @test dict_params[1] === dict_k
    @test dict_params[2] === dict_v

    @test fieldtypes(GlobalRef)[1] === Module
    @test fieldtypes(GlobalRef)[2] === Symbol
end

true
