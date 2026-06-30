using Test

# Issue #6196 / #5936: generated returned Expr(:call, callee,
# Expr(:parameters, Expr(:kw, ...)), args...) should dispatch through keyword
# argument binding.

function generated_keyword_expr_target_6196(x; y=1, z=2)
    x + y * z
end

@generated function generated_keyword_expr_6196(x)
    return Expr(
        :call,
        :generated_keyword_expr_target_6196,
        Expr(:parameters, Expr(:kw, :y, 5), Expr(:kw, :z, 3)),
        :x,
    )
end

@testset "generated returned keyword Expr call (Issue #6196)" begin
    @test generated_keyword_expr_target_6196(4; y=5, z=3) == 19
    @test generated_keyword_expr_6196(4) == 19
    @test generated_keyword_expr_6196(10) == 25
end

generated_keyword_expr_target_6196(4; y=5, z=3) == 19 &&
    generated_keyword_expr_6196(4) == 19 &&
    generated_keyword_expr_6196(10) == 25
