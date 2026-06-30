using Test

@testset "eval Expr(:tuple, ...) (Issue #5927)" begin
    @test eval(Expr(:tuple, 1, 2, 3)) == (1, 2, 3)
    @test eval(Expr(:tuple, Expr(:call, :+, 1, 1), 3)) == (2, 3)
    @test eval(Expr(:tuple)) == ()
end

true
