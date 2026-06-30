function keyword_function_default_vararg_where_contract_8367(f, segs::T...; norm=identity) where {T}
    return norm(f(0.5))
end

function keyword_function_default_vararg_where_override_8367(f, segs::T...; norm=abs) where {T}
    return norm(f(0.5))
end

f8367(x) = x * x

keyword_function_default_vararg_where_contract_8367(f8367, 0.0, 1.0) == 0.25 &&
    keyword_function_default_vararg_where_contract_8367(f8367, 0.0, 1.0; norm = x -> x + 1) == 1.25 &&
    keyword_function_default_vararg_where_override_8367(x -> -x, 0.0, 1.0) == 0.5
