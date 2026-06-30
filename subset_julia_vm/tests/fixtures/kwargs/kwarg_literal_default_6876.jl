# Issue #6876: a keyword argument whose default is an *array literal*
# (`x=[1,2]`) or a *tuple literal* (`x=(1,2)`) must bind to the literal value
# when the keyword is omitted -- not to `0`.
#
# Root cause: a source array literal parses to `Expr::ArrayLiteral` (a tuple to
# `Expr::TupleLiteral`), not the folded `Literal::Array` variant, so the
# pre-evaluated kwarg-default fast path (`eval_literal_default`) fell through to
# its `Value::I64(0)` fallback. The fix routes array/tuple-literal defaults
# through per-call body re-evaluation (matching upstream's per-call semantics:
# each omitted-keyword call gets a fresh array).
#
# Verified against upstream Julia 1.12.6 before implementation.

using Test

# --- array-literal defaults bind to the literal ------------------------------
f_arr_6876(; x=[1.0, 2.0]) = x
g_int_6876(; y=[1, 2]) = y
n_arr_6876(; z=[[1, 2], [3, 4]]) = z

@testset "kwargs_literal_default_6876: array-literal defaults bind correctly" begin
    @test f_arr_6876() == [1.0, 2.0]
    @test g_int_6876() == [1, 2]
    @test n_arr_6876() == [[1, 2], [3, 4]]
    @test length(f_arr_6876()) == 2
end

# --- per-call freshness: a mutated default must not leak across calls ---------
push_default_6876(; y=[1, 2]) = (push!(y, 3); y)

@testset "kwargs_literal_default_6876: array-literal default is fresh per call" begin
    @test push_default_6876() == [1, 2, 3]
    @test push_default_6876() == [1, 2, 3]
    @test push_default_6876() == [1, 2, 3]
end

# --- empty typed array-literal default ---------------------------------------
empty_default_6876(; v=Float64[]) = (push!(v, 1.0); v)

@testset "kwargs_literal_default_6876: empty typed array-literal default" begin
    @test empty_default_6876() == [1.0]
    @test empty_default_6876() == [1.0]
end

# --- comprehension default ---------------------------------------------------
comp_default_6876(; c=[i * i for i in 1:3]) = c

@testset "kwargs_literal_default_6876: comprehension default" begin
    @test comp_default_6876() == [1, 4, 9]
    @test comp_default_6876() == [1, 4, 9]
end

# --- tuple-literal defaults bind to the literal ------------------------------
t_tuple_6876(; t=(1, 2)) = t
m_tuple_6876(; p=(1, "a", 3.0)) = p

@testset "kwargs_literal_default_6876: tuple-literal defaults bind correctly" begin
    @test t_tuple_6876() == (1, 2)
    @test m_tuple_6876() == (1, "a", 3.0)
end

# --- controls: explicit pass + scalar default still work ---------------------
ctrl_6876(; x=[1.0, 2.0]) = x

@testset "kwargs_literal_default_6876: controls" begin
    @test ctrl_6876(x=[5.0]) == [5.0]
    @test ctrl_6876() == [1.0, 2.0]
end

true
