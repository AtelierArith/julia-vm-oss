using Test

# Issue #5936: generated bodies that construct an Expr and explicitly return it
# should evaluate the returned expression, matching Julia's staged-function result.

@generated function generated_return_expr_eval_5936(::Val{N}) where N
    ex = :(0 + 0)
    for i in 1:N
        ex = :($ex + $i)
    end
    return ex
end

@testset "generated returned Expr eval compatibility (Issue #5936)" begin
    @test generated_return_expr_eval_5936(Val(3)) == 6
    @test generated_return_expr_eval_5936(Val(5)) == 15
end

generated_return_expr_eval_5936(Val(3)) == 6 &&
    generated_return_expr_eval_5936(Val(5)) == 15
