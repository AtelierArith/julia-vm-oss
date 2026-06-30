# Test that typeof(haskey(...)) returns Bool, not Int64 (Issue #3473)

using Test

@testset "dict_haskey_typeof: haskey returns Bool, not Int64" begin
    d = Dict("a" => 1, "b" => 2)
    @test typeof(haskey(d, "a")) == Bool
    @test typeof(haskey(d, "z")) == Bool
    @test haskey(d, "a") == true
    @test haskey(d, "z") == false
end

true
