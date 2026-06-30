macro macro_body_foldr_lambda_7584()
    @assert true
    result = foldr((clause, tail) -> clause[1], [(:ok, :ignored)]; init=nothing)
    return result === :ok ? :(true) : :(false)
end

@macro_body_foldr_lambda_7584
