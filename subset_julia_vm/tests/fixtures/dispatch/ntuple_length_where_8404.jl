ntuple_len8404(xs::NTuple{N}, marker) where {N} = N
ntuple_pair8404(xs::NTuple{N, T}) where {N, T} = (N, T)

function ntuple_length_where_contract_8404()
    n1 = ntuple_len8404((1, 2), nothing)
    n2 = ntuple_len8404(("a", "b", "c"), nothing)
    p = ntuple_pair8404((3, 4, 5))

    n1 == 2 && n2 == 3 && p[1] == 3 && p[2] == Int64
end

ntuple_length_where_contract_8404()
