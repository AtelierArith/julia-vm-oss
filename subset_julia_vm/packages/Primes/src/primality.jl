function isprime(n::Int64)::Bool
    if n < 2
        return false
    end
    if n == 2
        return true
    end
    if n % 2 == 0
        return false
    end
    if n == 3
        return true
    end
    if n % 3 == 0
        return false
    end
    k = 5
    while k * k <= n
        if n % k == 0 || n % (k + 2) == 0
            return false
        end
        k += 6
    end
    return true
end
