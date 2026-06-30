using Test

macro where_bound_typevar_7924()
    esc(:(Tuple{S} where {T, S<:T}))
end

macro where_bound_typevar_expr_7924()
    esc(Expr(:where, Expr(:curly, :Tuple, :S), :T, Expr(:<:, :S, :T)))
end

@testset "macro-returned where keeps typevars referenced by bounds (Issue #7924)" begin
    @test string(@where_bound_typevar_7924()) == "Tuple{S} where {T, S<:T}"
    @test string(@where_bound_typevar_expr_7924()) == "Tuple{S} where {T, S<:T}"
end

true
