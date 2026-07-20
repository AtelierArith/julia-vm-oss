struct GaussKronrod
    x
    w
    wg
end

function GaussKronrod(::Type{T}) where {T<:Real}
    return GaussKronrod(QuadGK.cachedrule(T, 7)...)
end

const gk_float64 = GaussKronrod(QuadGK.kronrod(Float64, 7)...)
GaussKronrod(::Type{Float64}) = gk_float64

countevals(g::GaussKronrod) = 15

function _eval_rule(g::GaussKronrod, f, a_::SVector{1}, b_::SVector{1}, norm)
    a = a_[1]
    b = b_[1]
    T = float(typeof(a))
    c = (a + b) * T(0.5)
    delta = (b - a) * T(0.5)

    fx0 = f(SVector(c))
    I = fx0 * g.w[end]
    Ip = fx0 * g.wg[end]
    for i in 1:length(g.x)-1
        deltax = delta * g.x[i]
        fx = f(SVector(c + deltax)) + f(SVector(c - deltax))
        I += fx * g.w[i]
        if iseven(i)
            Ip += fx * g.wg[i >> 1]
        end
    end
    I *= delta
    Ip *= delta
    return I, norm(I - Ip), 1
end
