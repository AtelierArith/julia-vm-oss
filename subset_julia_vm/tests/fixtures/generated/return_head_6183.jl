using Test

# Issue #6183 / #5936: generated returned Expr eval should support
# Expr(:return, value_expr), which Julia accepts as a generated result.

@generated function generated_return_head_6183(x)
    return Expr(:return, Expr(:call, :+, :x, 3))
end

@testset "generated returned Expr(:return) eval (Issue #6183)" begin
    @test generated_return_head_6183(4) == 7
    @test generated_return_head_6183(10) == 13
end

generated_return_head_6183(4) == 7 &&
    generated_return_head_6183(10) == 13
