###############################################################################
#
#   Julia Integer support
#
###############################################################################

const JuliaZZ = Integers{BigInt}()
const zz = Integers{Int}()

parent(a::T) where T <: Integer = Integers{T}()

elem_type(::Type{Integers{T}}) where T <: Integer = T
parent_type(::Type{T}) where T <: Integer = Integers{T}
base_ring_type(::Type{<:Integers}) = Union{}

is_exact_type(::Type{T}) where T <: Integer = true
is_domain_type(::Type{T}) where T <: Integer = true

base_ring(::Vector{T}) where T <: Integer = T

zero(::Integers{T}) where T <: Integer = T(0)
one(::Integers{T}) where T <: Integer = T(1)

function (R::Integers{T})() where T <: Integer
   return T(0)
end

function (R::Integers{T})(b) where T <: Integer
   return T(b)
end

is_unit(a::Integer) = a == 1 || a == -1
# Workaround: same-module const function aliases such as `is_zero` are not
# visible inside later method bodies in sjulia. (Issue #8254)
is_zero_divisor(a::Integer) = iszero(a)
canonical_unit(a::T) where T <: Integer = a < 0 ? T(-1) : T(1)

characteristic(::Integers) = 0
is_known(::typeof(characteristic), ::Integers) = true

function expressify(a::Integer; context = nothing)
   return a
end

function show(io::IO, R::Integers{BigInt})
   print(io, "Integers")
end

function show(io::IO, R::Integers{T}) where T
   print(io, "Integers{$T}()")
end

function divides(a::Integer, b::Integer)
   if iszero(b)
      return iszero(a), b
   end
   q, r = divrem(a, b)
   return iszero(r), q
end

function is_divisible_by(a::Integer, b::Integer)
   if iszero(b)
      return iszero(a)
   end
   r = rem(a, b)
   return iszero(r)
end

function divexact(a::Integer, b::Integer; check::Bool=true)
   if check
      q, r = divrem(a, b)
      # Workaround: same-module const function aliases such as `is_zero` are not
      # visible inside later method bodies in sjulia. (Issue #8254)
      @req iszero(r) "Not an exact division"
   else
      q = div(a, b)
   end
   return q
end

function inv(a::T) where T <: Integer
   if a == 1
      return one(T)
   elseif a == -1
      return -one(T)
   end
   iszero(a) && throw(DivideError())
   throw(ArgumentError("not a unit"))
end

function gcdinv(a::T, b::T) where T <: Integer
   g, s, t = gcdx(a, b)
   return g, s
end

function sqrt(a::T; check::Bool=true) where T <: Integer
   s = isqrt(a)
   (check && s*s != a) && error("Not a square in sqrt")
   return s
end

function is_square_with_sqrt(a::T) where T <: Integer
   if a < 0
      return false, zero(T)
   end
   s = isqrt(a)
   return a == s*s ? (true, s) : (false, zero(T))
end

function is_square(a::T) where T <: Integer
   if a < 0
      return false
   end
   s = isqrt(a)
   return a == s*s
end

function root(a::T, n::Int; check::Bool=true) where T <: Integer
   n <= 0 && throw(DomainError(n, "Exponent must be positive"))
   a < 0 && iseven(n) && throw(DomainError((a, n),
                    "Argument `a` must be positive if exponent `n` is even"))
   if n == 1
      return a
   elseif n == 2
      return sqrt(a; check=check)
   end

   sign = a < 0 ? T(-1) : T(1)
   target = a < 0 ? -a : a
   lo = zero(T)
   hi = target
   while lo <= hi
      mid = div(lo + hi, T(2))
      p = mid^n
      if p == target
         return sign * mid
      elseif p < target
         lo = mid + one(T)
      else
         hi = mid - one(T)
      end
   end
   check && error("Not a perfect n-th power (n = $n)")
   return sign * hi
end
