struct Factorization
    bases::Vector
    exps::Vector
end

# Mirror upstream Primes.jl display: `factor(360)` shows as `2^3 ⋅ 3^2 ⋅ 5`
# (exponent 1 is omitted; factors joined by " ⋅ "). The empty factorization of 1
# prints as `1`. Issue #7171.
function Base.show(io::IO, f::Factorization)
    if isempty(f.bases)
        print(io, "1")
        return
    end
    for i in 1:length(f.bases)
        i == 1 || print(io, " ⋅ ")
        print(io, f.bases[i])
        if f.exps[i] != 1
            print(io, "^", f.exps[i])
        end
    end
end

Base.show(io::IO, ::MIME"text/plain", f::Factorization) = show(io, f)

function _factorization_length(f::Factorization)
    return length(f.bases)
end

function _factorization_base(f::Factorization, i::Int64)
    return f.bases[i]
end

function _factorization_exp(f::Factorization, i::Int64)
    return f.exps[i]
end

function _factor_int(n::Int64)
    bases = Int64[]
    exps = Int64[]
    if n == 0
        push!(bases, 0)
        push!(exps, 1)
        return Factorization(bases, exps)
    end
    if n < 0
        push!(bases, -1)
        push!(exps, 1)
        n = -n
    end
    if n == 1
        return Factorization(bases, exps)
    end
    d = 2
    while d * d <= n
        if n % d == 0
            exp = 0
            while n % d == 0
                exp += 1
                n = div(n, d)
            end
            push!(bases, d)
            push!(exps, exp)
        end
        d += 1
    end
    if n > 1
        push!(bases, n)
        push!(exps, 1)
    end
    return Factorization(bases, exps)
end

function factor(n::Int64)
    return _factor_int(n)
end

function factor(::Type{Vector}, n::Int64)
    result = Int64[]
    if n == 0
        return result
    end
    if n < 0
        n = -n
    end
    d = 2
    while d * d <= n
        while n % d == 0
            push!(result, d)
            n = div(n, d)
        end
        d += 1
    end
    if n > 1
        push!(result, n)
    end
    return result
end

function factor(::Type{Set}, n::Int64)
    if n < 0
        n = -n
    end
    result = Set{Int64}()
    if n <= 1
        return result
    end
    d = 2
    while d * d <= n
        if n % d == 0
            push!(result, d)
            while n % d == 0
                n = div(n, d)
            end
        end
        d += 1
    end
    if n > 1
        push!(result, n)
    end
    return result
end

function eachfactor(n::Int64)
    f = factor(n)
    len = _factorization_length(f)
    result = []
    i = 1
    while i <= len
        push!(result, (_factorization_base(f, i), _factorization_exp(f, i)))
        i += 1
    end
    return result
end

function prodfactors(f::Factorization)
    len = _factorization_length(f)
    result = 1
    i = 1
    while i <= len
        p = _factorization_base(f, i)
        e = _factorization_exp(f, i)
        result *= p ^ e
        i += 1
    end
    return result
end

function prodfactors(v::Vector)
    result = 1
    i = 1
    while i <= length(v)
        result *= v[i]
        i += 1
    end
    return result
end
