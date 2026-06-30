using Test

# Issue #6190 / #5936: eval(Expr(:copyast, QuoteNode(ex))) should return
# the quoted AST payload as data, matching Julia.

const GENERATED_COPYAST_EXPECTED_6190 = Expr(:call, :+, :x, 6)

@generated function generated_copyast_expr_head_6190(x)
    return Expr(:copyast, QuoteNode(Expr(:call, :+, :x, 6)))
end

@testset "generated/eval Expr(:copyast) head (Issue #6190)" begin
    local ex = Expr(:copyast, QuoteNode(Expr(:call, :+, :x, 6)))
    @test eval(ex) == GENERATED_COPYAST_EXPECTED_6190
    @test generated_copyast_expr_head_6190(7) == GENERATED_COPYAST_EXPECTED_6190
    @test generated_copyast_expr_head_6190(10) == GENERATED_COPYAST_EXPECTED_6190
end

eval(Expr(:copyast, QuoteNode(Expr(:call, :+, :x, 6)))) ==
    GENERATED_COPYAST_EXPECTED_6190 &&
    generated_copyast_expr_head_6190(7) == GENERATED_COPYAST_EXPECTED_6190 &&
    generated_copyast_expr_head_6190(10) == GENERATED_COPYAST_EXPECTED_6190
