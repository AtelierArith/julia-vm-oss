using Test

x = 7

@testset "eval Expr(:string, ...) (Issue #5931)" begin
    @test eval(Expr(:string, "x=", 1)) == "x=1"
    @test eval(Expr(:string, "n=", :x)) == "n=7"
end

true
