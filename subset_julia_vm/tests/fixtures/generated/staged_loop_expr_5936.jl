using Test

# Issue #5936: the parent staging-driver reproduction builds an Expr in the
# generated body with a loop and returns it for runtime evaluation.

@generated function generated_staged_loop_expr_5936(::Val{N}) where N
    ex = :(0 + 0)
    for i in 1:N
        ex = :($ex + $i)
    end
    return ex
end

@testset "generated staged loop Expr reproduction (Issue #5936)" begin
    @test generated_staged_loop_expr_5936(Val(3)) == 6
    @test generated_staged_loop_expr_5936(Val(5)) == 15
end

generated_staged_loop_expr_5936(Val(3)) == 6 &&
    generated_staged_loop_expr_5936(Val(5)) == 15
