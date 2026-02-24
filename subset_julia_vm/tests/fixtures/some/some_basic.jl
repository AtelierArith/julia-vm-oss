using Test

@testset "Some construction" begin
    s = Some(42)
    @test s.value == 42
end

@testset "something function" begin
    @test something(Some(5)) == 5
    @test something(nothing, 10) == 10
    @test something(5) == 5
    @test something(nothing, nothing, 30) == 30
end

true
