###############################################################################
#
#   Dense univariate polynomial MVP
#
###############################################################################

struct GenericPolyRing{T <: RingElement, R <: Ring} <: PolyRing{T}
   base_ring::R
   variable::Symbol
end

struct GenericPoly{T <: RingElement, R <: Ring} <: PolyRingElem{T}
   coeffs::Vector
   parent::GenericPolyRing{T, R}
end

function _poly_symbol(name::Symbol)::Symbol
   return name
end

function _poly_symbol(name::AbstractString)::Symbol
   return Symbol(name)
end

function _poly_symbol(name::Char)::Symbol
   return Symbol(string(name))
end

function polynomial_ring(R::Integers{T}, name::Symbol) where T <: Integer
   P = GenericPolyRing{T, typeof(R)}(R, _poly_symbol(name))
   return P, gen(P)
end

function polynomial_ring(R::Integers{T}, name::AbstractString) where T <: Integer
   P = GenericPolyRing{T, typeof(R)}(R, _poly_symbol(name))
   return P, gen(P)
end

function polynomial_ring(R::Integers{T}, name::Char) where T <: Integer
   P = GenericPolyRing{T, typeof(R)}(R, _poly_symbol(name))
   return P, gen(P)
end

function polynomial_ring(R::Rationals{T}, name::Symbol) where T <: Integer
   P = GenericPolyRing{Rational{T}, typeof(R)}(R, _poly_symbol(name))
   return P, gen(P)
end

function polynomial_ring(R::Rationals{T}, name::AbstractString) where T <: Integer
   P = GenericPolyRing{Rational{T}, typeof(R)}(R, _poly_symbol(name))
   return P, gen(P)
end

function polynomial_ring(R::Rationals{T}, name::Char) where T <: Integer
   P = GenericPolyRing{Rational{T}, typeof(R)}(R, _poly_symbol(name))
   return P, gen(P)
end

function parent(p::GenericPoly)
   return p.parent
end

function base_ring(P::GenericPolyRing)
   return P.base_ring
end

function base_ring(p::GenericPoly)
   return base_ring(parent(p))
end

elem_type(::Type{GenericPolyRing{T, R}}) where {T <: RingElement, R <: Ring} = GenericPoly{T, R}
parent_type(::Type{GenericPoly{T, R}}) where {T <: RingElement, R <: Ring} = GenericPolyRing{T, R}
base_ring_type(::Type{GenericPolyRing{T, R}}) where {T <: RingElement, R <: Ring} = R

function elem_type(P::GenericPolyRing)
   return typeof(gen(P))
end

function parent_type(p::GenericPoly)
   return typeof(parent(p))
end

function _poly_base_name(R::Integers{BigInt})
   return "integers"
end

function _poly_base_name(R::Rationals{BigInt})
   return "rationals"
end

function _poly_base_name(R)
   return string(R)
end

function _poly_ring_to_string(P::GenericPolyRing)
   return "Univariate polynomial ring in " * string(P.variable) * " over " * _poly_base_name(P.base_ring)
end

function show(io::IO, P::GenericPolyRing)
   print(io, _poly_ring_to_string(P))
end

function _poly_coerce(R::Integers{T}, c) where T <: Integer
   return T(c)
end

function _poly_coerce(R::Rationals{T}, c::Integer) where T <: Integer
   # Workaround: `Rational{T}(x)` can construct malformed Rational{BigInt}
   # values when `T` comes from a parametric method. (Issue #8253)
   return Rational{T}(T(c), T(1))
end

function _poly_coerce(R::Rationals{T}, c::Rational) where T <: Integer
   return Rational{T}(T(numerator(c)), T(denominator(c)))
end

function _poly_zero_coeff(P::GenericPolyRing)
   return zero(P.base_ring)
end

function _poly_one_coeff(P::GenericPolyRing)
   return one(P.base_ring)
end

function _poly_normalize_coeffs(P::GenericPolyRing, coeffs)
   out = Any[]
   i = 1
   while i <= length(coeffs)
      push!(out, _poly_coerce(P.base_ring, coeffs[i]))
      i += 1
   end
   while length(out) > 0 && iszero(out[length(out)])
      pop!(out)
   end
   return out
end

function _poly_from_coeffs(P::GenericPolyRing{T, R}, coeffs) where {T <: RingElement, R <: Ring}
   return GenericPoly{T, R}(_poly_normalize_coeffs(P, coeffs), P)
end

function (P::GenericPolyRing)()
   return zero(P)
end

function (P::GenericPolyRing)(c)
   return _poly_from_coeffs(P, Any[c])
end

function (P::GenericPolyRing)(coeffs::Vector)
   return _poly_from_coeffs(P, coeffs)
end

function (P::GenericPolyRing)(p::GenericPoly)
   parent(p) === P || error("parents do not match")
   return p
end

function zero(P::GenericPolyRing)
   return _poly_from_coeffs(P, Any[])
end

function one(P::GenericPolyRing)
   return _poly_from_coeffs(P, Any[_poly_one_coeff(P)])
end

function gen(P::GenericPolyRing)
   return _poly_from_coeffs(P, Any[_poly_zero_coeff(P), _poly_one_coeff(P)])
end

function gens(P::GenericPolyRing)
   return Any[gen(P)]
end

function number_of_generators(P::GenericPolyRing)
   return 1
end

function symbols(P::GenericPolyRing)
   return Any[P.variable]
end

function degree(p::GenericPoly)
   return length(p.coeffs) - 1
end

function coeff(p::GenericPoly, n::Integer)
   n < 0 && error("coefficient index must be non-negative")
   i = Int(n) + 1
   if i <= length(p.coeffs)
      return p.coeffs[i]
   end
   return _poly_zero_coeff(parent(p))
end

function iszero(p::GenericPoly)
   return length(p.coeffs) == 0
end

function isone(p::GenericPoly)
   return length(p.coeffs) == 1 && isone(p.coeffs[1])
end

function ==(a::GenericPoly, b::GenericPoly)
   check_parent(a, b)
   if length(a.coeffs) != length(b.coeffs)
      return false
   end
   i = 1
   while i <= length(a.coeffs)
      a.coeffs[i] == b.coeffs[i] || return false
      i += 1
   end
   return true
end

function +(a::GenericPoly, b::GenericPoly)
   check_parent(a, b)
   P = parent(a)
   n = length(a.coeffs) > length(b.coeffs) ? length(a.coeffs) : length(b.coeffs)
   coeffs = Any[]
   i = 1
   while i <= n
      ca = i <= length(a.coeffs) ? a.coeffs[i] : _poly_zero_coeff(P)
      cb = i <= length(b.coeffs) ? b.coeffs[i] : _poly_zero_coeff(P)
      push!(coeffs, ca + cb)
      i += 1
   end
   return _poly_from_coeffs(P, coeffs)
end

function +(a::GenericPoly, b::RingElement)
   return a + _poly_from_coeffs(parent(a), Any[b])
end

function +(a::RingElement, b::GenericPoly)
   return _poly_from_coeffs(parent(b), Any[a]) + b
end

+(a::GenericPoly, b::Integer) = a + _poly_from_coeffs(parent(a), Any[b])
+(a::Integer, b::GenericPoly) = _poly_from_coeffs(parent(b), Any[a]) + b
+(a::GenericPoly, b::Rational) = a + _poly_from_coeffs(parent(a), Any[b])
+(a::Rational, b::GenericPoly) = _poly_from_coeffs(parent(b), Any[a]) + b

function -(a::GenericPoly)
   P = parent(a)
   coeffs = Any[]
   i = 1
   while i <= length(a.coeffs)
      push!(coeffs, -a.coeffs[i])
      i += 1
   end
   return _poly_from_coeffs(P, coeffs)
end

function -(a::GenericPoly, b::GenericPoly)
   return a + (-b)
end

function -(a::GenericPoly, b::RingElement)
   return a - _poly_from_coeffs(parent(a), Any[b])
end

function -(a::RingElement, b::GenericPoly)
   return _poly_from_coeffs(parent(b), Any[a]) - b
end

-(a::GenericPoly, b::Integer) = a - _poly_from_coeffs(parent(a), Any[b])
-(a::Integer, b::GenericPoly) = _poly_from_coeffs(parent(b), Any[a]) - b
-(a::GenericPoly, b::Rational) = a - _poly_from_coeffs(parent(a), Any[b])
-(a::Rational, b::GenericPoly) = _poly_from_coeffs(parent(b), Any[a]) - b

function *(a::GenericPoly, b::GenericPoly)
   check_parent(a, b)
   P = parent(a)
   if iszero(a) || iszero(b)
      return zero(P)
   end
   coeffs = Any[]
   n = length(a.coeffs) + length(b.coeffs) - 1
   i = 1
   while i <= n
      push!(coeffs, _poly_zero_coeff(P))
      i += 1
   end
   i = 1
   while i <= length(a.coeffs)
      j = 1
      while j <= length(b.coeffs)
         idx = i + j - 1
         product = a.coeffs[i] * b.coeffs[j]
         if iszero(coeffs[idx])
            # Workaround: adding BigInt through an `Any` array zero slot widens
            # to Float64 in sjulia, so store the first product directly.
            # (Issue #8262)
            coeffs[idx] = product
         else
            # Workaround: adding BigInt through an `Any` array slot widens to
            # Float64 in sjulia; re-coerce the accumulator to the base ring.
            # (Issue #8262)
            coeffs[idx] = _poly_coerce(P.base_ring, coeffs[idx]) + product
         end
         j += 1
      end
      i += 1
   end
   return _poly_from_coeffs(P, coeffs)
end

function *(a::GenericPoly, b::RingElement)
   return a * _poly_from_coeffs(parent(a), Any[b])
end

function *(a::RingElement, b::GenericPoly)
   return _poly_from_coeffs(parent(b), Any[a]) * b
end

*(a::GenericPoly, b::Integer) = a * _poly_from_coeffs(parent(a), Any[b])
*(a::Integer, b::GenericPoly) = _poly_from_coeffs(parent(b), Any[a]) * b
*(a::GenericPoly, b::Rational) = a * _poly_from_coeffs(parent(a), Any[b])
*(a::Rational, b::GenericPoly) = _poly_from_coeffs(parent(b), Any[a]) * b

function ^(a::GenericPoly, n::Integer)
   n < 0 && error("negative polynomial powers are not supported")
   P = parent(a)
   result = one(P)
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

function evaluate(p::GenericPoly, x)
   result = _poly_zero_coeff(parent(p))
   i = length(p.coeffs)
   while i >= 1
      result = result * x + p.coeffs[i]
      i -= 1
   end
   return result
end

function derivative(p::GenericPoly)
   P = parent(p)
   if length(p.coeffs) <= 1
      return zero(P)
   end
   coeffs = Any[]
   i = 2
   while i <= length(p.coeffs)
      push!(coeffs, p.coeffs[i] * (i - 1))
      i += 1
   end
   return _poly_from_coeffs(P, coeffs)
end

function _poly_divrem_exact(a::GenericPoly, b::GenericPoly; check::Bool=true)
   check_parent(a, b)
   iszero(b) && throw(DivideError())
   P = parent(a)
   r = Any[]
   i = 1
   while i <= length(a.coeffs)
      push!(r, a.coeffs[i])
      i += 1
   end
   q = Any[]
   q_len = degree(a) >= degree(b) ? degree(a) - degree(b) + 1 : 0
   i = 1
   while i <= q_len
      push!(q, _poly_zero_coeff(P))
      i += 1
   end
   while length(r) >= length(b.coeffs) && length(r) > 0
      shift = length(r) - length(b.coeffs)
      c = divexact(r[length(r)], b.coeffs[length(b.coeffs)]; check=check)
      if iszero(q[shift + 1])
         # Workaround: adding BigInt through an `Any` array zero slot widens
         # to Float64 in sjulia, so store the first quotient term directly.
         # (Issue #8262)
         q[shift + 1] = c
      else
         # Workaround: adding BigInt through an `Any` array slot widens to
         # Float64 in sjulia; re-coerce the accumulator to the base ring.
         # (Issue #8262)
         q[shift + 1] = _poly_coerce(P.base_ring, q[shift + 1]) + c
      end
      new_r = Any[]
      k = 1
      while k <= length(r)
         if k > shift && k <= shift + length(b.coeffs)
            j = k - shift
            # Workaround: subtracting from a BigInt held in an `Any` array slot
            # can widen through numeric fallback; re-coerce before arithmetic
            # and rebuild the vector with `push!` because assigning back into an
            # existing `Any` slot can also widen BigInt to Float64. (Issue #8262)
            push!(new_r, _poly_coerce(P.base_ring, r[k]) - c * b.coeffs[j])
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
   if check && length(r) != 0
      error("Not an exact division")
   end
   return _poly_from_coeffs(P, q), _poly_from_coeffs(P, r)
end

function divexact(a::GenericPoly, b::GenericPoly; check::Bool=true)
   q, r = _poly_divrem_exact(a, b; check=check)
   return q
end

function _poly_term_string(c, pow::Int, var::Symbol)
   if pow == 0
      return string(c)
   elseif pow == 1
      if isone(c)
         return string(var)
      else
         return string(c) * "*" * string(var)
      end
   else
      if isone(c)
         return string(var) * "^" * string(pow)
      else
         return string(c) * "*" * string(var) * "^" * string(pow)
      end
   end
end

function _poly_to_string(p::GenericPoly)
   if iszero(p)
      return "0"
   end
   var = parent(p).variable
   s = ""
   first = true
   i = length(p.coeffs)
   while i >= 1
      c = p.coeffs[i]
      if !iszero(c)
         pow = i - 1
         if first
            if c < 0
               s *= "-"
               c = -c
            end
            s *= _poly_term_string(c, pow, var)
            first = false
         else
            if c < 0
               s *= " - "
               c = -c
            else
               s *= " + "
            end
            s *= _poly_term_string(c, pow, var)
         end
      end
      i -= 1
   end
   return s
end

function show(io::IO, p::GenericPoly)
   print(io, _poly_to_string(p))
end
