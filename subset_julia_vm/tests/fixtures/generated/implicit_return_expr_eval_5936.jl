using Test

# Issue #5936: a generated body can use its final expression as the staged
# expression result. The compatibility fallback should evaluate that staged Expr,
# not return the Expr object itself.

@generated function generated_implicit_return_expr_eval_5936(::Val{N}) where N
    ex = :(0 + 0)
    for i in 1:N
        ex = :($ex + $i)
    end
    ex
end

@generated function generated_implicit_expr_constructor_eval_5936()
    Expr(:call, :+, 20, 22)
end

@testset "generated implicit returned Expr eval compatibility (Issue #5936)" begin
    @test generated_implicit_return_expr_eval_5936(Val(3)) == 6
    @test generated_implicit_return_expr_eval_5936(Val(5)) == 15
    @test generated_implicit_expr_constructor_eval_5936() == 42
end

generated_implicit_return_expr_eval_5936(Val(3)) == 6 &&
    generated_implicit_return_expr_eval_5936(Val(5)) == 15 &&
    generated_implicit_expr_constructor_eval_5936() == 42
