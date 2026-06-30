function reflection_expr_fields_through_any_7614(ex)
    args_by_name = getfield(ex, :args)
    args_by_index = getfield(ex, 2)

    ex.head === :call &&
        getfield(ex, :head) === :call &&
        getfield(ex, 1) === :call &&
        length(args_by_name) == 3 &&
        args_by_name[1] === :+ &&
        args_by_index[1] === :+
end

reflection_expr_fields_through_any_7614(:(x + 1))
