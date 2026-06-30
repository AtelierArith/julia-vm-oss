using Test

macro macro_return_typed_expr_7628()
    esc(:(x::Int))
end

function macro_return_typed_expr_7628_f(x)
    @macro_return_typed_expr_7628
end

@testset "macro expansion lowers Expr(::) in value position (Issue #7628)" begin
    @test macro_return_typed_expr_7628_f(1) == 1
    @test_throws Exception macro_return_typed_expr_7628_f(1.5)
end

true
