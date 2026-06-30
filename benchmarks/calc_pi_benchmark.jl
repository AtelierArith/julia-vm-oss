# Estimate π using coprime probability
# P(gcd(a,b) = 1) = 6/π² → π = √(6/P)

function mygcd(a, b)
    while b != 0
        tmp = b
        b = a % b
        a = tmp
    end
    a
end

function calc_pi(N)
    cnt = 0
    for a in 1:N
        for b in 1:N
            if mygcd(a, b) == 1
                cnt += 1
            end
        end
    end
    prob = cnt / N / N
    sqrt(6.0 / prob)
end

# Benchmark for N=100
@time result = calc_pi(100)
println("N=100: π ≈ ", result)

# Benchmark for N=500
@time result = calc_pi(500)
println("N=500: π ≈ ", result)

# Benchmark for N=1000
@time result = calc_pi(1000)
println("N=1000: π ≈ ", result)
