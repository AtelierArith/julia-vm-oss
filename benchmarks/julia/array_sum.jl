# Array sum benchmark
# Tests loop performance and array access

function array_sum(arr)
    s = 0
    for i in 1:length(arr)
        s = s + arr[i]
    end
    s
end

# Benchmark entry point
function main()
    n = 100000
    arr = Int64[]
    for i in 1:n
        push!(arr, i)
    end
    result = array_sum(arr)
    println(result)
end

main()
