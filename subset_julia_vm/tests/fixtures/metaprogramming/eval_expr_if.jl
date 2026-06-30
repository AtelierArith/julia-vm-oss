using Test

@testset "eval Expr(:if, ...) (Issue #5929)" begin
    @test eval(Expr(:if, true, 10, 20)) == 10
    @test eval(Expr(:if, false, 10, 20)) == 20
    @test eval(Expr(:if, false, 10)) === nothing
    @test eval(:(true ? 1 : 2)) == 1
end

true
