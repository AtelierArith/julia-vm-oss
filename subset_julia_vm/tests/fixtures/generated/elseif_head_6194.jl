using Test

# Issue #6194 / #5936: eval(Expr(:elseif, ...)) should follow the same
# conditional semantics as Expr(:if, ...).

@generated function generated_elseif_expr_head_6194(::Val{B}) where B
    return Expr(:elseif, B, 10, 20)
end

@testset "generated/eval Expr(:elseif) head (Issue #6194)" begin
    @test eval(Expr(:elseif, true, 1, 2)) == 1
    @test eval(Expr(:elseif, false, 1, 2)) == 2
    @test generated_elseif_expr_head_6194(Val(true)) == 10
    @test generated_elseif_expr_head_6194(Val(false)) == 20
end

eval(Expr(:elseif, true, 1, 2)) == 1 &&
    eval(Expr(:elseif, false, 1, 2)) == 2 &&
    generated_elseif_expr_head_6194(Val(true)) == 10 &&
    generated_elseif_expr_head_6194(Val(false)) == 20
