using Test

dict_testset_haskey_global_6263 = Dict("b" => 2)

@testset "testset global Dict haskey stays dynamic" begin
    global dict_testset_haskey_global_6263

    @test haskey(dict_testset_haskey_global_6263, "b")

    dict_testset_haskey_global_6263 = 42
    @test dict_testset_haskey_global_6263 == 42
end

dict_testset_haskey_global_6263 == 42
