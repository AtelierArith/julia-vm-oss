# Sum of Squares - Type-Annotated Numeric Computation
#
# AoT で “完全静的” に落としやすい、単純な数値ループの例。
#
# Key features:
# - 型注釈付き関数（引数/戻り値）
# - Int64 ループ + Float64 accumulator
function sum_squares(n::Int64)::Float64
    result = 0.0
    for i in 1:n
        result += Float64(i * i)
    end
    return result
end

# Alternative: Using closed-form formula
# sum_squares_formula(n) = n * (n + 1) * (2n + 1) / 6
function sum_squares_formula(n::Int64)::Float64
    return Float64(n) * Float64(n + 1) * Float64(2 * n + 1) / 6.0
end

function main()
    # AoT 出力が実行可能になるように、ここで一度呼び出す
    x = sum_squares(100)
    y = sum_squares_formula(100)
    x == y
    true
end
