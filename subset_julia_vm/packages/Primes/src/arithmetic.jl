function radical(n::Int64)
    if n == 0
        return 0
    end
    if n < 0
        n = -n
    end
    result = 1
    d = 2
    while d * d <= n
        if n % d == 0
            result *= d
            while n % d == 0
                n = div(n, d)
            end
        end
        d += 1
    end
    if n > 1
        result *= n
    end
    return result
end

function totient(n::Int64)
    if n < 0
        n = -n
    end
    if n == 0
        return 0
    end
    result = n
    temp = n
    d = 2
    while d * d <= temp
        if temp % d == 0
            while temp % d == 0
                temp = div(temp, d)
            end
            result -= div(result, d)
        end
        d += 1
    end
    if temp > 1
        result -= div(result, temp)
    end
    return result
end

function divisors(n::Int64)
    if n < 0
        n = -n
    end
    if n == 0
        return Int64[]
    end
    small = Int64[]
    large = Int64[]
    d = 1
    while d * d <= n
        if n % d == 0
            push!(small, d)
            q = div(n, d)
            if q != d
                push!(large, q)
            end
        end
        d += 1
    end
    result = Int64[]
    i = 1
    while i <= length(small)
        push!(result, small[i])
        i += 1
    end
    j = length(large)
    while j >= 1
        push!(result, large[j])
        j -= 1
    end
    return result
end
