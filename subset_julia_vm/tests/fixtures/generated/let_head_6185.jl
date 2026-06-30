using Test

# Issue #6185 / #5936: generated returned Expr eval should support
# Expr(:let, binding..., body), with bindings scoped to the eval frame.

@generated function generated_let_expr_head_6185(x)
    return Expr(
        :let,
        Expr(:(=), :y, Expr(:call, :+, :x, 2)),
        Expr(:call, :*, :y, 3),
    )
end

@generated function generated_let_block_head_6185(x)
    return Expr(
        :let,
        Expr(:(=), :y, 2),
        Expr(
            :block,
            Expr(:(=), :z, Expr(:call, :+, :x, :y)),
            Expr(:call, :*, :z, 2),
        ),
    )
end

@testset "generated returned Expr(:let) eval (Issue #6185)" begin
    @test generated_let_expr_head_6185(5) == 21
    @test generated_let_expr_head_6185(10) == 36
    @test generated_let_block_head_6185(5) == 14
    @test generated_let_block_head_6185(10) == 24
end

generated_let_expr_head_6185(5) == 21 &&
    generated_let_expr_head_6185(10) == 36 &&
    generated_let_block_head_6185(5) == 14 &&
    generated_let_block_head_6185(10) == 24
