# Estimate π using coprime probability
# P(gcd(a,b) = 1) = 6/π² → π = √(6/P)
# Tests nested loops and integer operations

function mygcd(a, b)
    while b != 0
        tmp = b
        b = a % b
        a = tmp
    end
    a
end

function calc_pi(n)
    cnt = 0
    for a in 1:n
        for b in 1:n
            if mygcd(a, b) == 1
                cnt += 1
            end
        end
    end
    prob = cnt / n / n
    sqrt(6.0 / prob)
end

# Benchmark entry point
function main()
    n = 10000
    result = calc_pi(n)
    println(result)
end

main()
