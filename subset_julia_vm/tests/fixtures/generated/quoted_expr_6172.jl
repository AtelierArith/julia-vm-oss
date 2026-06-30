using Test

# Issue #6172 / #5936: @generated bodies can return a quoted Expr selected by
# generated-time control flow. The compatibility fallback must evaluate the
# unwrapped Expr against the runtime argument frame, not return the Expr object.

@generated function generated_quoted_expr_6172(x)
    x == Int64 ? :(x + 1) : :(0)
end

@testset "generated quoted Expr payload (Issue #6172)" begin
    @test generated_quoted_expr_6172(2) == 3
    @test generated_quoted_expr_6172(2.0) == 0
end

generated_quoted_expr_6172(2) == 3 && generated_quoted_expr_6172(2.0) == 0
