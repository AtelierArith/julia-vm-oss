module UnionAliasUserStructBinding8409

abstract type Ring end
abstract type RingElem end

const RingElement = Union{RingElem, Integer, Rational, AbstractFloat}

struct BaseRing <: Ring
end

struct Poly <: RingElem
end

struct Field{T <: RingElement, R <: Ring}
    base::R
end

struct Fraction{T <: RingElement, R <: Ring}
    parent::Field{T, R}
end

make(F::Field{T, R}) where {T <: RingElement, R <: Ring} = Fraction{T, R}(F)

end

F = UnionAliasUserStructBinding8409.Field{
    UnionAliasUserStructBinding8409.Poly,
    UnionAliasUserStructBinding8409.BaseRing,
}(UnionAliasUserStructBinding8409.BaseRing())

frac = UnionAliasUserStructBinding8409.make(F)
typeof(frac) === UnionAliasUserStructBinding8409.Fraction{
    UnionAliasUserStructBinding8409.Poly,
    UnionAliasUserStructBinding8409.BaseRing,
}
