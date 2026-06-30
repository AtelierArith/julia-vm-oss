# Fibonacci benchmark - recursive implementation
# Tests function call overhead and recursion performance

function fib(n::Int64)::Int64
    if n <= 1
        n
    else
        fib(n - 1) + fib(n - 2)
    end
end

# Benchmark entry point
function main()
    n = 30
    result = fib(n)
    println(result)
end

main()
