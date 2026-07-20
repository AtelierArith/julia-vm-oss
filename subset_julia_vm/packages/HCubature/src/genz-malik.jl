# Generic n-dimensional Genz-Malik cubature rule (n >= 2), following upstream
# HCubature.jl src/genz-malik.jl:
# A. C. Genz and A. A. Malik, "An adaptive algorithm for numeric integration
# over an N-dimensional rectangular region," J. Comput. Appl. Math.,
# vol. 6 (no. 4), 295-302 (1980).
#
# Unlike upstream, the dimension is carried as an `Int` field instead of a
# `Val{n}` type parameter: mixing concrete `Val{k}` methods with a generic
# `Val{n}` method risks the dispatch mis-selection tracked in Issue #8537.

"""
    combos(k, lambda, n)

Return an array of `SVector{n}` of all n-component vectors with `k` components
equal to `lambda` and other components equal to zero.
"""
function combos(k::Integer, lambda::T, n::Integer) where {T<:Number}
    p = []
    for c in Combinatorics.combinations(1:n, k)
        v = fill(zero(T), n)
        for i in c
            v[i] = lambda
        end
        push!(p, SVector{n,T}(Tuple(v)))
    end
    return p
end

"""
    signcombos(k, lambda, n)

Return an array of `SVector{n}` of all n-component vectors with `k` components
equal to `±lambda` and other components equal to zero (all possible signs).
"""
function signcombos(k::Integer, lambda::T, n::Integer) where {T<:Number}
    p = []
    twok = 1 << k
    for c in Combinatorics.combinations(1:n, k)
        v = fill(zero(T), n)
        for i in c
            v[i] = lambda
        end
        push!(p, SVector{n,T}(Tuple(v)))
        # use a gray code to flip one sign at a time (upstream point order)
        graycode = 0
        for s in 1:(twok - 1)
            graycode2 = xor(s, s >> 1)
            graycomp = c[trailing_zeros(xor(graycode, graycode2)) + 1]
            graycode = graycode2
            v[graycomp] = -v[graycomp]
            push!(p, SVector{n,T}(Tuple(v)))
        end
    end
    return p
end

struct GenzMalik
    n::Int
    p
    w
    wp
end

function _GenzMalik(n::Int, ::Type{T}) where {T<:Real}
    n < 2 && throw(ArgumentError("invalid dimension $n: GenzMalik rule requires dimension >= 2"))

    lambda4 = sqrt(9 / T(10))
    lambda2 = sqrt(9 / T(70))
    lambda3 = lambda4
    lambda5 = sqrt(9 / T(19))

    two_to_n = 1 << n
    w1 = two_to_n * ((12824 - 9120 * n + 400 * n^2) / T(19683))
    w2 = two_to_n * (980 / T(6561))
    w3 = two_to_n * ((1820 - 400 * n) / T(19683))
    w4 = two_to_n * (200 / T(19683))
    w5 = 6859 / T(19683)
    wp4 = two_to_n * (25 / T(729))
    wp3 = two_to_n * ((265 - 100 * n) / T(1458))
    wp2 = two_to_n * (245 / T(486))
    wp1 = two_to_n * ((729 - 950 * n + 50 * n^2) / T(729))

    p2 = combos(1, lambda2, n)
    p3 = combos(1, lambda3, n)
    p4 = signcombos(2, lambda4, n)
    p5 = signcombos(n, lambda5, n)

    return GenzMalik(n, (p2, p3, p4, p5), (w1, w2, w3, w4, w5), (wp1, wp2, wp3, wp4))
end

GenzMalik(n::Integer, ::Type{T}) where {T<:Real} = _GenzMalik(Int(n), T)
GenzMalik(n::Integer) = _GenzMalik(Int(n), Float64)

countevals(g::GenzMalik) = 1 + 4 * g.n + 2 * g.n * (g.n - 1) + (1 << g.n)

function _eval_rule(g::GenzMalik, f, a, b, norm)
    n = g.n
    c = 0.5 .* (a .+ b)
    delta = 0.5 .* (b .- a)
    volume = prod(delta)

    f1 = f(c)
    f2 = zero(f1)
    f3 = zero(f1)
    twelvef1 = 12 * f1
    maxdivdiff = zero(norm(f1))
    divdiff = fill(maxdivdiff, n)

    for i in 1:n
        p2 = delta .* g.p[1][i]
        f2i = f(c + p2) + f(c - p2)
        p3 = delta .* g.p[2][i]
        f3i = f(c + p3) + f(c - p3)
        f2 += f2i
        f3 += f3i
        # fourth divided difference: f3i-2f1 - 7*(f2i-2f1),
        # where 7 = (lambda3/lambda2)^2 [see van Dooren and de Ridder]
        divdiff[i] = norm(f3i + twelvef1 - 7 * f2i)
    end

    f4 = zero(f1)
    for p in g.p[3]
        f4 += f(c .+ delta .* p)
    end

    f5 = zero(f1)
    for p in g.p[4]
        f5 += f(c .+ delta .* p)
    end

    I = volume * (g.w[1] * f1 + g.w[2] * f2 + g.w[3] * f3 + g.w[4] * f4 + g.w[5] * f5)
    Ip = volume * (g.wp[1] * f1 + g.wp[2] * f2 + g.wp[3] * f3 + g.wp[4] * f4)
    E = norm(I - Ip)

    # choose axis with the largest fourth divided difference to subdivide next
    kdivide = 1
    deltaf = E / (10^n * volume)
    for i in 1:n
        d = divdiff[i] - maxdivdiff
        if d > deltaf
            kdivide = i
            maxdivdiff = divdiff[i]
        elseif abs(d) <= deltaf && abs(delta[i]) > abs(delta[kdivide])
            kdivide = i
        end
    end

    return I, E, kdivide
end
