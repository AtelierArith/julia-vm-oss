using Test

@testset "fresh TypeVar identity" begin
    a = TypeVar(:T)
    b = TypeVar(:T)

    @test a.name === :T
    @test a.lb === Union{}
    @test a.ub === Any

    @test a === a
    @test !(a === b)
    @test objectid(a) != objectid(b)
    @test isequal(a, a)
    @test !isequal(a, b)

    bounded = TypeVar(:S, Union{}, Integer)
    @test bounded.name === :S
    @test bounded.lb === Union{}
    @test bounded.ub === Integer
end

true
