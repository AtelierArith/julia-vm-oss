using Test

macro quote_expr_args_7898()
    ex = :(f(x))
    esc(Expr(:quote, ex.args))
end

@testset "macro-returned quote can rematerialize Expr.args arrays (Issue #7898)" begin
    args = @quote_expr_args_7898()
    @test length(args) == 2
    @test args[1] === :f
    @test args[2] === :x
    @test typeof(args) == Vector{Any}
end

true
