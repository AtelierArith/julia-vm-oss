using Test

# Issue #7844: a macro that returns a `where` type (Expr(:where, body, var...))
# must bind each introduced inner type variable as a runtime TypeVar(:var) fed to
# UnionAll, while still resolving caller-bound type params in the body
# dynamically. Previously the macro-return AST->IR converter had no Expr(:where,
# ...) arm, so the introduced inner S was lowered as an ordinary variable
# (UndefVarError) instead of being bound as TypeVar(:S). The fix routes the body
# through the curly/DynamicTypeConstruct path (so caller-bound T resolves) and
# binds each introduced var in a `let` to TypeVar(:S) passed to UnionAll.

macro tuple_where()
    esc(:(Tuple{T,S} where S))
end

function tuple_type_for(x::T) where T
    @tuple_where()
end

# Constructed-Expr form must behave identically to the quote form.
macro tuple_where_expr()
    esc(Expr(:where, Expr(:curly, :Tuple, :T, :S), :S))
end

tuple_type_for_expr(x::T) where T = @tuple_where_expr()

# A `where` whose only variable is the introduced one (no caller param in the
# body) — the body still references a caller binding via a concrete element.
macro vector_where()
    esc(:(Tuple{Vector{V}} where V))
end

vector_where_for(x::T) where T = @vector_where()

# NOTE: deliberately no top-level `T = Int64` here — a global binding named the
# same as a method `where` parameter collides in DynamicTypeConstruct
# (Issue #7847), which is independent of this macro-return fix.

@testset "macro-returned where binds inner TypeVar (Issue #7844)" begin
    @test tuple_type_for(1) == (Tuple{Int64, S} where S)
    @test tuple_type_for(1.0) == (Tuple{Float64, S} where S)
    @test string(tuple_type_for(1)) == "Tuple{Int64, S} where S"
    @test string(tuple_type_for(1.0)) == "Tuple{Float64, S} where S"

    @test tuple_type_for_expr(1) == (Tuple{Int64, S} where S)
    @test tuple_type_for_expr(1.0) == (Tuple{Float64, S} where S)

    @test vector_where_for(1) == (Tuple{Vector{V}} where V)
    @test string(vector_where_for(1)) == "Tuple{Vector{V}} where V"
end

true
