using Test

@testset "count(f, range) preserves range element types" begin
    @test count(x -> typeof(x) == Int64, 1:5) == 5
    @test count(x -> typeof(x) == Float64, 1:5) == 0
    @test count(x -> typeof(x) == Float64, 1.0:0.5:3.0) == 5
    @test count(x -> x > 2, 1:5) == 3
end

true
