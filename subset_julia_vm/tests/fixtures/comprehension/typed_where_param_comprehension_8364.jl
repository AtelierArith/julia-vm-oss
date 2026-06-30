function typed_where_param_comprehension_contract_8364(::Type{T}) where {T}
    b = T[n / sqrt(4n^2 - one(T)) for n = 1:2]
    return typeof(b) == Vector{T} &&
        eltype(b) == T &&
        abs(b[1] - T(1 / sqrt(3))) < T(1e-14)
end

typed_where_param_comprehension_contract_8364(Float64)
