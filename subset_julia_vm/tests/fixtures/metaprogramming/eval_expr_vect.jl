using Test

@testset "eval Expr(:vect, ...) (Issue #5928)" begin
    @test eval(Expr(:vect, 1, 2, 3)) == [1, 2, 3]
    @test eval(Expr(:vect, Expr(:call, :+, 1, 1), 3)) == [2, 3]

    empty = eval(Expr(:vect))
    @test isempty(empty)
    @test eltype(empty) == Any
end

true
