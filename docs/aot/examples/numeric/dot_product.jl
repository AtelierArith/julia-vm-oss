# Dot Product - Array Numeric Computation
#
# AoT で “完全静的” に落としやすい、1D 配列 + インデックスループの例。

function dot_product(a::Vector{Float64}, b::Vector{Float64})::Float64
    n = length(a)
    result = 0.0
    for i in 1:n
        result += a[i] * b[i]
    end
    return result
end

function vector_norm(v::Vector{Float64})::Float64
    return sqrt(dot_product(v, v))
end

function cosine_similarity(a::Vector{Float64}, b::Vector{Float64})::Float64
    norm_a = vector_norm(a)
    norm_b = vector_norm(b)
    if norm_a == 0.0 || norm_b == 0.0
        return 0.0
    end
    return dot_product(a, b) / (norm_a * norm_b)
end

function main()
    a = [1.0, 2.0, 3.0]
    b = [4.0, 5.0, 6.0]
    d = dot_product(a, b)      # 32.0
    n = vector_norm(a)
    c = cosine_similarity(a, b)
    d
    n
    c
    true
end
