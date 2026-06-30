function sum_range_typed(n::Int64)::Int64
    s::Int64 = 0
    for i in 1:n
        s = s + i
    end
    s
end

function sum_range_zero_based_typed(n::Int64)::Int64
    s::Int64 = 0
    for i in 0:(n - 1)
        s = s + i
    end
    s
end

println(sum_range_typed(100) == 5050)
println(sum_range_zero_based_typed(100) == 4950)
"true\ntrue\n"
