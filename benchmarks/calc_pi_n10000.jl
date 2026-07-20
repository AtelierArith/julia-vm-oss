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

result = calc_pi(10000)
println("N=10000: π ≈ ", result)
