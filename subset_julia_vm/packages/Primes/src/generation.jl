function _make_sieve(hi::Int64)
    sieve = Bool[]
    i = 1
    while i <= hi
        push!(sieve, true)
        i += 1
    end
    sieve[1] = false
    return sieve
end

function primesmask(hi::Int64)
    if hi < 2
        return Bool[]
    end
    sieve = _make_sieve(hi)
    i = 2
    while i * i <= hi
        if sieve[i]
            j = i * i
            while j <= hi
                sieve[j] = false
                j += i
            end
        end
        i += 1
    end
    return sieve
end

function primesmask(lo::Int64, hi::Int64)
    if hi < lo
        return Bool[]
    end
    full = primesmask(hi)
    lo_clamped = lo < 1 ? 1 : lo
    result = Bool[]
    i = lo_clamped
    while i <= hi
        push!(result, full[i])
        i += 1
    end
    return result
end

function primes(hi::Int64)
    if hi < 2
        return Int64[]
    end
    sieve = primesmask(hi)
    result = Int64[]
    i = 1
    while i <= hi
        if sieve[i]
            push!(result, i)
        end
        i += 1
    end
    return result
end

function primes(lo::Int64, hi::Int64)
    if hi < lo || hi < 2
        return Int64[]
    end
    all_primes = primes(hi)
    result = Int64[]
    i = 1
    while i <= length(all_primes)
        if all_primes[i] >= lo
            push!(result, all_primes[i])
        end
        i += 1
    end
    return result
end

function nextprime(n::Int64)
    if n <= 2
        return 2
    end
    candidate = n % 2 == 0 ? n + 1 : n
    while !isprime(candidate)
        candidate += 2
    end
    return candidate
end

function prevprime(n::Int64)
    if n <= 2
        throw(ArgumentError("no prime less than or equal to 1"))
    end
    if n == 3
        return 2
    end
    candidate = n % 2 == 0 ? n - 1 : n
    while candidate >= 2 && !isprime(candidate)
        candidate -= 2
    end
    if candidate < 2
        throw(ArgumentError("no prime less than or equal to 1"))
    end
    return candidate
end

function prime(i::Int64)
    if i < 1
        throw(ArgumentError("index must be positive"))
    end
    count = 0
    candidate = 1
    while count < i
        candidate += 1
        if isprime(candidate)
            count += 1
        end
    end
    return candidate
end
