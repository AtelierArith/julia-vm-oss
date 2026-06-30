module AliasWhereBoundDispatch8406

abstract type Ring end
abstract type RingElem end

const RingElement = Union{RingElem, Integer, Rational, AbstractFloat}

struct BaseRing <: Ring
end

struct GenericPolyRing{T <: RingElement, R <: Ring}
    base::R
end

function make(P::GenericPolyRing{T, R}, coeffs) where {T <: RingElement, R <: Ring}
    return length(coeffs)
end

end

P = AliasWhereBoundDispatch8406.GenericPolyRing{BigInt, AliasWhereBoundDispatch8406.BaseRing}(
    AliasWhereBoundDispatch8406.BaseRing(),
)

AliasWhereBoundDispatch8406.make(P, Any[0, 1]) == 2
