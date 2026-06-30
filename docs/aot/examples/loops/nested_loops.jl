# Nested Loops (1D) Examples
#
# AoT で “完全静的” に落としやすい、ネストしたループ（1D配列中心）の例。

function small_convolution(data::Vector{Float64}, kernel::Vector{Float64})::Vector{Float64}
    n = length(data)
    k = length(kernel)
    result = zeros(n)

    for i in 1:n
        sum = 0.0
        for j in 1:k
            idx = i - j + 1
            if idx >= 1 && idx <= n
                sum += data[idx] * kernel[j]
            end
        end
        result[i] = sum
    end

    return result
end

function pairwise_dot_sum(a::Vector{Float64}, b::Vector{Float64})::Float64
    # 例: a と b の全ペア積を合計（O(n^2)）
    n = length(a)
    total = 0.0
    for i in 1:n
        for j in 1:n
            total += a[i] * b[j]
        end
    end
    return total
end

function main()
    x = [1.0, 2.0, 3.0, 4.0]
    k = [1.0, 1.0]
    small_convolution(x, k)
    pairwise_dot_sum(x, x)
    true
end

