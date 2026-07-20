## BigInt operation benchmark
## Measures fib, factorial, cumulative sum, and chained-add patterns.
## Used to track Issue #9105 (Rc::try_unwrap allocation reduction).
##
## Run:
##   ./target/release/sjulia benchmarks/vm_bigint_ops.jl

function fib_big(n)
    a, b = big(0), big(1)
    for i in 2:n
        a, b = b, a + b
    end
    b
end

function fact_big(n)
    r = big(1)
    for i in 2:n
        r = r * big(i)
    end
    r
end

function sum_big(n)
    s = big(0)
    for i in 1:n
        s = s + big(i)
    end
    s
end

# Chain: second operand of each + is a fresh temporary (count==1),
# so Rc::try_unwrap succeeds on the rhs.
function chain_add(n)
    r = big(0)
    for i in 1:n
        r = r + big(i) + big(i)
    end
    r
end

# Warmup
_ = fib_big(50)
_ = fact_big(20)
_ = sum_big(100)
_ = chain_add(50)

t0 = time_ns()
for _ in 1:200
    x = fib_big(500)
end
t1 = time_ns()
println("fib_big(500) x200: ", div(t1 - t0, 1_000_000), " ms")

t0 = time_ns()
for _ in 1:50
    x = fact_big(500)
end
t1 = time_ns()
println("fact_big(500) x50: ", div(t1 - t0, 1_000_000), " ms")

t0 = time_ns()
for _ in 1:20
    x = sum_big(1000)
end
t1 = time_ns()
println("sum_big(1000) x20: ", div(t1 - t0, 1_000_000), " ms")

t0 = time_ns()
for _ in 1:20
    x = chain_add(500)
end
t1 = time_ns()
println("chain_add(500) x20: ", div(t1 - t0, 1_000_000), " ms")

# Correctness assertions
@assert fib_big(100) == big(354224848179261915075)
@assert fact_big(10) == big(3628800)
@assert sum_big(10) == big(55)
@assert chain_add(5) == big(30)
println("All correctness checks passed.")
