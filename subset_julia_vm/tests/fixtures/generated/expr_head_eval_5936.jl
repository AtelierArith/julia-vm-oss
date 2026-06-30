using Test

# Issue #5936: generated fallback Expr evaluation should compose with the
# eval mini-interpreter heads that were split out as #5927-#5932.

const GENERATED_REF_DATA_5936 = [10, 20, 30]

@generated function generated_tuple_expr_head_5936(::Val{N}) where N
    return Expr(:tuple, N, N + 1)
end

@generated function generated_vect_expr_head_5936()
    return Expr(:vect, 1, Expr(:call, :+, 1, 2))
end

@generated function generated_if_expr_head_5936(::Val{B}) where B
    return Expr(:if, B, Expr(:call, :+, 10, 1), Expr(:call, :+, 20, 2))
end

@generated function generated_curly_expr_head_5936(::Val{N}) where N
    return Expr(:curly, :Val, N)
end

@generated function generated_string_expr_head_5936(::Val{N}) where N
    return Expr(:string, "n=", N)
end

@generated function generated_ref_expr_head_5936()
    return Expr(:ref, :GENERATED_REF_DATA_5936, 2)
end

@testset "generated returned Expr eval heads (Issue #5936)" begin
    @test generated_tuple_expr_head_5936(Val(3)) == (3, 4)
    @test generated_vect_expr_head_5936() == [1, 3]
    @test generated_if_expr_head_5936(Val(true)) == 11
    @test generated_if_expr_head_5936(Val(false)) == 22
    @test generated_curly_expr_head_5936(Val(5)) == Val{5}
    @test generated_string_expr_head_5936(Val(7)) == "n=7"
    @test generated_ref_expr_head_5936() == 20
end

generated_tuple_expr_head_5936(Val(3)) == (3, 4) &&
    generated_vect_expr_head_5936() == [1, 3] &&
    generated_if_expr_head_5936(Val(true)) == 11 &&
    generated_if_expr_head_5936(Val(false)) == 22 &&
    generated_curly_expr_head_5936(Val(5)) == Val{5} &&
    generated_string_expr_head_5936(Val(7)) == "n=7" &&
    generated_ref_expr_head_5936() == 20
