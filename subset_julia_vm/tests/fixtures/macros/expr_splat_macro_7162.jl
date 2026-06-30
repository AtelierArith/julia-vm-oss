macro expr_splat_macro_7162(x, y)
    names = [esc(x), esc(y)]
    Expr(
        :block,
        Expr(:(=), esc(x), 7),
        Expr(:(=), esc(y), 8),
        Expr(:vect, names...),
    )
end

result_7162 = @expr_splat_macro_7162 a_7162 b_7162

a_7162 == 7 && b_7162 == 8 && result_7162 == [7, 8]
