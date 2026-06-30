###############################################################################
#
#   Fraction fields and residue rings MVP
#
###############################################################################

struct SimpleFracField{T <: RingElement, R <: Ring} <: FracField{T}
   base_ring::R
end

struct SimpleFrac{T <: RingElement, R <: Ring} <: FracElem{T}
   num::T
   den::T
   parent::SimpleFracField{T, R}
end

function fraction_field(R::GenericPolyRing)
   return SimpleFracField{typeof(gen(R)), typeof(R)}(R)
end

function parent(a::SimpleFrac)
   return a.parent
end

function base_ring(F::SimpleFracField)
   return F.base_ring
end

function base_ring(a::SimpleFrac)
   return base_ring(parent(a))
end

function elem_type(F::SimpleFracField)
   return typeof(_frac_make(F, zero(base_ring(F)), one(base_ring(F))))
end

elem_type(::Type{SimpleFracField{T, R}}) where {T <: RingElement, R <: Ring} = SimpleFrac{T, R}
parent_type(::Type{SimpleFrac{T, R}}) where {T <: RingElement, R <: Ring} = SimpleFracField{T, R}
base_ring_type(::Type{SimpleFracField{T, R}}) where {T <: RingElement, R <: Ring} = R

function parent_type(a::SimpleFrac)
   return typeof(parent(a))
end

function _frac_coerce(F::SimpleFracField, x::GenericPoly)
   parent(x) === base_ring(F) || error("Could not coerce to fraction")
   return x
end

function _frac_coerce(F::SimpleFracField, x)
   return _poly_from_coeffs(base_ring(F), Any[x])
end

function _frac_make(F::SimpleFracField{T, R}, num, den) where {T <: RingElement, R <: Ring}
   n = _frac_coerce(F, num)
   d = _frac_coerce(F, den)
   iszero(d) && throw(DivideError())
   return SimpleFrac{T, R}(n, d, F)
end

function (F::SimpleFracField)(num, den)
   return _frac_make(F, num, den)
end

function (F::SimpleFracField)(num)
   return _frac_make(F, num, one(base_ring(F)))
end

function numerator(a::SimpleFrac, canonicalise::Bool=true)
   return a.num
end

function denominator(a::SimpleFrac, canonicalise::Bool=true)
   return a.den
end

zero(F::SimpleFracField) = _frac_make(F, zero(base_ring(F)), one(base_ring(F)))
one(F::SimpleFracField) = _frac_make(F, one(base_ring(F)), one(base_ring(F)))

iszero(a::SimpleFrac) = iszero(numerator(a, false))
isone(a::SimpleFrac) = numerator(a, false) == denominator(a, false)
is_unit(a::SimpleFrac) = !iszero(a)

function ==(a::SimpleFrac, b::SimpleFrac)
   check_parent(a, b)
   return numerator(a, false) * denominator(b, false) ==
          numerator(b, false) * denominator(a, false)
end

function +(a::SimpleFrac, b::SimpleFrac)
   check_parent(a, b)
   n = numerator(a, false) * denominator(b, false) +
       numerator(b, false) * denominator(a, false)
   d = denominator(a, false) * denominator(b, false)
   # Workaround: callable fraction-field parent dispatch fails for
   # `F(num, den)` in sjulia, so internal arithmetic routes through `_frac_make`.
   # (Issue #8264)
   return _frac_make(parent(a), n, d)
end

function -(a::SimpleFrac)
   # Workaround: callable fraction-field parent dispatch fails for
   # `F(num, den)` in sjulia, so internal arithmetic routes through `_frac_make`.
   # (Issue #8264)
   return _frac_make(parent(a), -numerator(a, false), denominator(a, false))
end

function -(a::SimpleFrac, b::SimpleFrac)
   return a + (-b)
end

function *(a::SimpleFrac, b::SimpleFrac)
   check_parent(a, b)
   # Workaround: callable fraction-field parent dispatch fails for
   # `F(num, den)` in sjulia, so internal arithmetic routes through `_frac_make`.
   # (Issue #8264)
   return _frac_make(parent(a),
                     numerator(a, false) * numerator(b, false),
                     denominator(a, false) * denominator(b, false))
end

function _frac_to_string(a::SimpleFrac)
   if isone(denominator(a, false))
      return _poly_to_string(numerator(a, false))
   end
   return "(" * _poly_to_string(numerator(a, false)) * ")/(" *
          _poly_to_string(denominator(a, false)) * ")"
end

function _frac_field_to_string(F::SimpleFracField)
   return "Fraction field of " * _poly_ring_to_string(base_ring(F))
end

function show(io::IO, F::SimpleFracField)
   print(io, _frac_field_to_string(F))
end

function show(io::IO, a::SimpleFrac)
   print(io, _frac_to_string(a))
end

struct SimpleResidueRing{T <: Integer, R <: Ring} <: ResidueRing{T}
   base_ring::R
   modulus::T
end

struct SimpleResidue{T <: Integer, R <: Ring} <: ResElem{T}
   value::T
   parent::SimpleResidueRing{T, R}
end

function residue_ring(R::Integers{T}, n::Integer) where T <: Integer
   m = T(n)
   iszero(m) && error("Modulus must be nonzero")
   m < 0 && (m = -m)
   return (SimpleResidueRing{T, typeof(R)}(R, m),)
end

function parent(a::SimpleResidue)
   return a.parent
end

function base_ring(R::SimpleResidueRing)
   return R.base_ring
end

function modulus(R::SimpleResidueRing)
   return R.modulus
end

function modulus(a::SimpleResidue)
   return modulus(parent(a))
end

function data(a::SimpleResidue)
   return a.value
end

function lift(a::SimpleResidue)
   return data(a)
end

function elem_type(R::SimpleResidueRing{T, S}) where {T <: Integer, S <: Ring}
   return SimpleResidue{T, S}
end

elem_type(::Type{SimpleResidueRing{T, S}}) where {T <: Integer, S <: Ring} = SimpleResidue{T, S}
parent_type(::Type{SimpleResidue{T, S}}) where {T <: Integer, S <: Ring} = SimpleResidueRing{T, S}
base_ring_type(::Type{SimpleResidueRing{T, S}}) where {T <: Integer, S <: Ring} = S

function parent_type(a::SimpleResidue)
   return typeof(parent(a))
end

function _residue_normalize(R::SimpleResidueRing{T, S}, x) where {T <: Integer, S <: Ring}
   v = mod(T(x), modulus(R))
   return SimpleResidue{T, S}(v, R)
end

function (R::SimpleResidueRing)(x)
   return _residue_normalize(R, x)
end

zero(R::SimpleResidueRing) = R(0)
one(R::SimpleResidueRing) = R(1)

iszero(a::SimpleResidue) = iszero(data(a))
isone(a::SimpleResidue) = data(a) == 1 || a == one(parent(a))
is_unit(a::SimpleResidue) = isone(gcd(data(a), modulus(a)))
is_zero_divisor(a::SimpleResidue) = !is_unit(a)
characteristic(R::SimpleResidueRing) = modulus(R)
is_known(::typeof(characteristic), R::SimpleResidueRing) = true

function ==(a::SimpleResidue, b::SimpleResidue)
   check_parent(a, b)
   return data(a) == data(b)
end

function +(a::SimpleResidue, b::SimpleResidue)
   check_parent(a, b)
   return parent(a)(data(a) + data(b))
end

function -(a::SimpleResidue)
   return parent(a)(-data(a))
end

function -(a::SimpleResidue, b::SimpleResidue)
   return a + (-b)
end

function *(a::SimpleResidue, b::SimpleResidue)
   check_parent(a, b)
   return parent(a)(data(a) * data(b))
end

function _residue_ring_to_string(R::SimpleResidueRing)
   return "Residue ring of integers modulo " * string(modulus(R))
end

function _residue_to_string(a::SimpleResidue)
   return string(data(a))
end

function show(io::IO, R::SimpleResidueRing)
   print(io, _residue_ring_to_string(R))
end

function show(io::IO, a::SimpleResidue)
   print(io, _residue_to_string(a))
end

struct SimplePolyResidueRing{T <: RingElement, R <: Ring} <: ResidueRing{T}
   base_ring::R
   modulus::T
end

struct SimplePolyResidue{T <: RingElement, R <: Ring} <: ResElem{T}
   value::T
   parent::SimplePolyResidueRing{T, R}
end

function residue_ring(P::GenericPolyRing, f::GenericPoly)
   parent(f) === P || error("Modulus must belong to the polynomial ring")
   iszero(f) && error("Modulus must be nonzero")
   isone(coeff(f, degree(f))) || error("Polynomial residue MVP requires a monic modulus")
   Q = SimplePolyResidueRing{typeof(gen(P)), typeof(P)}(P, f)
   return Q, Q(gen(P))
end

function parent(a::SimplePolyResidue)
   return a.parent
end

function base_ring(Q::SimplePolyResidueRing)
   return Q.base_ring
end

function modulus(Q::SimplePolyResidueRing)
   return Q.modulus
end

function modulus(a::SimplePolyResidue)
   return modulus(parent(a))
end

function data(a::SimplePolyResidue)
   return a.value
end

function lift(a::SimplePolyResidue)
   return data(a)
end

function elem_type(Q::SimplePolyResidueRing{T, R}) where {T <: RingElement, R <: Ring}
   return SimplePolyResidue{T, R}
end

function elem_type(Q::SimplePolyResidueRing)
   return typeof(zero(Q))
end

elem_type(::Type{SimplePolyResidueRing{T, R}}) where {T <: RingElement, R <: Ring} = SimplePolyResidue{T, R}
parent_type(::Type{SimplePolyResidue{T, R}}) where {T <: RingElement, R <: Ring} = SimplePolyResidueRing{T, R}
base_ring_type(::Type{SimplePolyResidueRing{T, R}}) where {T <: RingElement, R <: Ring} = R

function parent_type(a::SimplePolyResidue)
   return typeof(parent(a))
end

function _poly_residue_coerce(Q::SimplePolyResidueRing, x::GenericPoly)
   parent(x) === base_ring(Q) || error("Could not coerce to polynomial residue ring")
   return x
end

function _poly_residue_coerce(Q::SimplePolyResidueRing, x)
   return _poly_from_coeffs(base_ring(Q), Any[x])
end

function _poly_residue_reduce(Q::SimplePolyResidueRing, x)
   P = base_ring(Q)
   p = _poly_residue_coerce(Q, x)
   m = modulus(Q)
   r = Any[]
   i = 1
   while i <= length(p.coeffs)
      push!(r, p.coeffs[i])
      i += 1
   end
   while length(r) >= length(m.coeffs) && length(r) > 0
      shift = length(r) - length(m.coeffs)
      c = r[length(r)]
      new_r = Any[]
      k = 1
      while k <= length(r)
         if k > shift && k <= shift + length(m.coeffs)
            j = k - shift
            push!(new_r, _poly_coerce(P.base_ring, r[k]) - c * m.coeffs[j])
         else
            push!(new_r, r[k])
         end
         k += 1
      end
      r = new_r
      while length(r) > 0 && iszero(r[length(r)])
         pop!(r)
      end
   end
   return _poly_from_coeffs(P, r)
end

function _poly_residue_make(Q::SimplePolyResidueRing{T, R}, x) where {T <: RingElement, R <: Ring}
   return SimplePolyResidue{T, R}(_poly_residue_reduce(Q, x), Q)
end

function (Q::SimplePolyResidueRing)(x)
   return _poly_residue_make(Q, x)
end

zero(Q::SimplePolyResidueRing) = Q(zero(base_ring(Q)))
one(Q::SimplePolyResidueRing) = Q(one(base_ring(Q)))

iszero(a::SimplePolyResidue) = iszero(data(a))
isone(a::SimplePolyResidue) = isone(data(a))

function ==(a::SimplePolyResidue, b::SimplePolyResidue)
   check_parent(a, b)
   return data(a) == data(b)
end

==(a::SimplePolyResidue, b::GenericPoly) = a == parent(a)(b)
==(a::GenericPoly, b::SimplePolyResidue) = parent(b)(a) == b
==(a::SimplePolyResidue, b::RingElement) = a == parent(a)(b)
==(a::RingElement, b::SimplePolyResidue) = parent(b)(a) == b
==(a::SimplePolyResidue, b::Integer) = a == parent(a)(b)
==(a::Integer, b::SimplePolyResidue) = parent(b)(a) == b
==(a::SimplePolyResidue, b::Rational) = a == parent(a)(b)
==(a::Rational, b::SimplePolyResidue) = parent(b)(a) == b

function +(a::SimplePolyResidue, b::SimplePolyResidue)
   check_parent(a, b)
   return parent(a)(data(a) + data(b))
end

+(a::SimplePolyResidue, b::GenericPoly) = a + parent(a)(b)
+(a::GenericPoly, b::SimplePolyResidue) = parent(b)(a) + b
+(a::SimplePolyResidue, b::RingElement) = a + parent(a)(b)
+(a::RingElement, b::SimplePolyResidue) = parent(b)(a) + b
+(a::SimplePolyResidue, b::Integer) = a + parent(a)(b)
+(a::Integer, b::SimplePolyResidue) = parent(b)(a) + b
+(a::SimplePolyResidue, b::Rational) = a + parent(a)(b)
+(a::Rational, b::SimplePolyResidue) = parent(b)(a) + b

function -(a::SimplePolyResidue)
   return parent(a)(-data(a))
end

function -(a::SimplePolyResidue, b::SimplePolyResidue)
   return a + (-b)
end

-(a::SimplePolyResidue, b::GenericPoly) = a - parent(a)(b)
-(a::GenericPoly, b::SimplePolyResidue) = parent(b)(a) - b
-(a::SimplePolyResidue, b::RingElement) = a - parent(a)(b)
-(a::RingElement, b::SimplePolyResidue) = parent(b)(a) - b
-(a::SimplePolyResidue, b::Integer) = a - parent(a)(b)
-(a::Integer, b::SimplePolyResidue) = parent(b)(a) - b
-(a::SimplePolyResidue, b::Rational) = a - parent(a)(b)
-(a::Rational, b::SimplePolyResidue) = parent(b)(a) - b

function *(a::SimplePolyResidue, b::SimplePolyResidue)
   check_parent(a, b)
   return parent(a)(data(a) * data(b))
end

*(a::SimplePolyResidue, b::GenericPoly) = a * parent(a)(b)
*(a::GenericPoly, b::SimplePolyResidue) = parent(b)(a) * b
*(a::SimplePolyResidue, b::RingElement) = a * parent(a)(b)
*(a::RingElement, b::SimplePolyResidue) = parent(b)(a) * b
*(a::SimplePolyResidue, b::Integer) = a * parent(a)(b)
*(a::Integer, b::SimplePolyResidue) = parent(b)(a) * b
*(a::SimplePolyResidue, b::Rational) = a * parent(a)(b)
*(a::Rational, b::SimplePolyResidue) = parent(b)(a) * b

function ^(a::SimplePolyResidue, n::Integer)
   n < 0 && error("negative residue powers are not supported")
   result = one(parent(a))
   base = a
   e = Int(n)
   while e > 0
      if isodd(e)
         result = result * base
      end
      e = div(e, 2)
      if e > 0
         base = base * base
      end
   end
   return result
end

function _poly_residue_ring_to_string(Q::SimplePolyResidueRing)
   return "Residue ring of " * _poly_ring_to_string(base_ring(Q)) *
          " modulo " * _poly_to_string(modulus(Q))
end

function _poly_residue_to_string(a::SimplePolyResidue)
   return _poly_to_string(data(a))
end

function show(io::IO, Q::SimplePolyResidueRing)
   print(io, _poly_residue_ring_to_string(Q))
end

function show(io::IO, a::SimplePolyResidue)
   print(io, _poly_residue_to_string(a))
end
