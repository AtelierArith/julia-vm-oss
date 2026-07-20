using Test

@testset "Int64 div signed-min overflow throws DivideError (Issue #8896)" begin
    @test_throws DivideError div(typemin(Int64), Int64(-1))
    @test div(typemin(Int64), Int64(1)) == typemin(Int64)
    @test div(typemin(Int64) + Int64(1), Int64(-1)) == typemax(Int64)
end

@testset "Int128 div signed-min overflow throws DivideError" begin
    @test_throws DivideError div(typemin(Int128), Int128(-1))
end

true
