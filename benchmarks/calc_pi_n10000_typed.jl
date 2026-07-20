# Estimate π using coprime probability (fully typed)
# P(gcd(a,b) = 1) = 6/π² → π = √(6/P)

function mygcd(a::Int64, b::Int64)::Int64
    while b != 0
        tmp::Int64 = b
        b = a % b
        a = tmp
    end
    a
end

function calc_pi(N::Int64)::Float64
    cnt::Int64 = 0
    for a in 1:N
        for b in 1:N
            if mygcd(a, b) == 1
                cnt += 1
            end
        end
    end
    prob::Float64 = cnt / N / N
    sqrt(6.0 / prob)
end

result::Float64 = calc_pi(10000)
println("N=10000: π ≈ ", result)
