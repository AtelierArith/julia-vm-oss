###############################################################################
#
#   Julia Rational support
#
###############################################################################

const JuliaQQ = Rationals{BigInt}()
const qq = Rationals{Int}()

parent(a::Rational{T}) where T <: Integer = Rationals{T}()

elem_type(::Type{Rationals{T}}) where T <: Integer = Rational{T}
parent_type(::Type{Rational{T}}) where T <: Integer = Rationals{T}
base_ring_type(::Type{Rationals{T}}) where T <: Integer = Integers{T}
base_ring(a::Rationals{T}) where T <: Integer = Integers{T}()

is_exact_type(::Type{Rational{T}}) where T <: Integer = true
is_domain_type(::Type{Rational{T}}) where T <: Integer = true

function zero(::Rationals{T}) where T <: Integer
   # Workaround: `Rational{T}(x)` can construct malformed Rational{BigInt}
   # values when `T` comes from a parametric method. (Issue #8253)
   return Rational{T}(T(0), T(1))
end

function one(::Rationals{T}) where T <: Integer
   # Workaround: `Rational{T}(x)` can construct malformed Rational{BigInt}
   # values when `T` comes from a parametric method. (Issue #8253)
   return Rational{T}(T(1), T(1))
end

# Workaround: same-module const function aliases such as `is_zero` are not
# visible inside later method bodies in sjulia. (Issue #8254)
is_unit(a::Rational) = !iszero(a)
is_zero_divisor(a::Rational) = iszero(a)
canonical_unit(a::Rational) = iszero(a) ? one(a) : a

function numerator(a::Rational, canonicalise::Bool=true)
   return Base.numerator(a)
end

function denominator(a::Rational, canonicalise::Bool=true)
   return Base.denominator(a)
end

characteristic(::Rationals) = 0
is_known(::typeof(characteristic), ::Rationals) = true

function expressify(a::Rational; context = nothing)
   n = numerator(a)
   d = denominator(a)
   if isone(d)
      return n
   else
      return Expr(:call, ://, n, d)
   end
end

function show(io::IO, R::Rationals{BigInt})
   print(io, "Rationals")
end

function show(io::IO, R::Rationals{T}) where T
   print(io, "Rationals{$T}()")
end

function divides(a::T, b::T) where T <: Rational
   if iszero(b)
      return false, T(0)
   else
      return true, divexact(a, b; check=false)
   end
end

function divexact(a::Rational, b::Integer; check::Bool=true)
   # Workaround: `//(::Rational, ::Rational)` / rational exact-division shapes
   # fail in sjulia; `/` preserves the upstream-visible rational result here.
   # (Issue #8255)
   return a / b
end

function divexact(a::Integer, b::Rational; check::Bool=true)
   # Workaround: `//(::Rational, ::Rational)` / rational exact-division shapes
   # fail in sjulia; `/` preserves the upstream-visible rational result here.
   # (Issue #8255)
   return a / b
end

function divexact(a::Rational, b::Rational; check::Bool=true)
   # Workaround: `//(::Rational, ::Rational)` / rational exact-division shapes
   # fail in sjulia; `/` preserves the upstream-visible rational result here.
   # (Issue #8255)
   return a / b
end

function sqrt(a::Rational{T}; check::Bool=true) where T <: Integer
   return sqrt(numerator(a, false); check=check)//sqrt(denominator(a, false); check=check)
end

function is_square(a::Rational{T}) where T <: Integer
   return is_square(numerator(a)) && is_square(denominator(a))
end

function is_square_with_sqrt(a::Rational{T}) where T <: Integer
   f1, s1 = is_square_with_sqrt(numerator(a))
   if !f1
      return false, zero(T)
   end
   f2, s2 = is_square_with_sqrt(denominator(a))
   if !f2
      return false, zero(T)
   end
   return true, s1//s2
end

function root(a::Rational{T}, n::Int; check::Bool=true) where T <: Integer
   num = root(numerator(a, false), n; check=check)
   den = root(denominator(a, false), n; check=check)
   return num//den
end

function (R::Rationals{T})() where T <: Integer
   # Workaround: `Rational{T}(x)` can construct malformed Rational{BigInt}
   # values when `T` comes from a parametric method. (Issue #8253)
   return Rational{T}(T(0), T(1))
end

function (R::Rationals{T})(b) where T <: Integer
   # Workaround: `Rational{T}(x)` can construct malformed Rational{BigInt}
   # values when `T` comes from a parametric method. (Issue #8253)
   return Rational{T}(T(b), T(1))
end

function (R::Rationals{T})(b::Integer, c::Integer) where T <: Integer
   return Rational{T}(b, c)
end

fraction_field(R::Integers{T}) where T <: Integer = Rationals{T}()
