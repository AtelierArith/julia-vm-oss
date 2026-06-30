###############################################################################
#
#   Dense matrix MVP
#
###############################################################################

struct GenericMatrix{T <: NCRingElement} <: MatElem{T}
   base_ring::NCRing
   nrows::Int
   ncols::Int
   entries::Vector
end

function _matrix_coerce(R::Integers{T}, x) where T <: Integer
   return T(x)
end

function _matrix_coerce(R::Rationals{T}, x::Integer) where T <: Integer
   # Workaround: `Rational{T}(x)` can construct malformed Rational{BigInt}
   # values when `T` comes from a parametric method. (Issue #8253)
   return Rational{T}(T(x), T(1))
end

function _matrix_coerce(R::Rationals{T}, x::Rational) where T <: Integer
   return Rational{T}(T(numerator(x)), T(denominator(x)))
end

function _matrix_coerce(R::GenericPolyRing, x::GenericPoly)
   parent(x) === R || error("Could not coerce to matrix base ring")
   return x
end

function _matrix_coerce(R::GenericPolyRing, x)
   return _poly_from_coeffs(R, Any[x])
end

function _matrix_coerce(R, x)
   return R(x)
end

function _matrix_index(A::GenericMatrix, i::Int, j::Int)
   (1 <= i <= A.nrows && 1 <= j <= A.ncols) || error("matrix index out of bounds")
   return (i - 1) * A.ncols + j
end

function _matrix_from_flat(R::NCRing, r::Int, c::Int, entries)
   (r < 0 || c < 0) && error("Dimensions must be non-negative")
   length(entries) == r * c || throw(ErrorConstrDimMismatch(r, c, length(entries)))
   out = Any[]
   i = 1
   while i <= length(entries)
      # Workaround: typed `Vector{BigInt}` / `Matrix{BigInt}` stores read back
      # as Float64 in sjulia, so dense matrices use flat Any storage and coerce
      # every value at the package boundary. (Issue #8266)
      push!(out, _matrix_coerce(R, entries[i]))
      i += 1
   end
   return GenericMatrix{elem_type(R)}(R, r, c, out)
end

function matrix_space(R::NCRing, r::Int, c::Int; cached::Bool = true)
   return MatSpace{elem_type(R)}(R, r, c)
end

function matrix(R::NCRing, r::Int, c::Int, entries::Vector)
   return _matrix_from_flat(R, r, c, entries)
end

function matrix(R::NCRing, entries::Matrix)
   return matrix(R, size(entries, 1), size(entries, 2), entries)
end

function matrix(R::NCRing, r::Int, c::Int, entries::Matrix)
   size(entries, 1) == r && size(entries, 2) == c || throw(ErrorConstrDimMismatch(r, c, size(entries, 1), size(entries, 2)))
   flat = Any[]
   i = 1
   while i <= r
      j = 1
      while j <= c
         push!(flat, entries[i, j])
         j += 1
      end
      i += 1
   end
   return _matrix_from_flat(R, r, c, flat)
end

function (S::MatSpace)()
   return zero_matrix(base_ring(S), number_of_rows(S), number_of_columns(S))
end

function (S::MatSpace)(entries)
   return matrix(base_ring(S), number_of_rows(S), number_of_columns(S), entries)
end

function (S::MatSpace)(entries::Matrix)
   return matrix(base_ring(S), number_of_rows(S), number_of_columns(S), entries)
end

function (S::MatSpace)(entries::Vector)
   return matrix(base_ring(S), number_of_rows(S), number_of_columns(S), entries)
end

function parent(A::GenericMatrix)
   return matrix_space(base_ring(A), number_of_rows(A), number_of_columns(A))
end

function base_ring(S::MatSpace)
   return S.base_ring
end

function base_ring(A::GenericMatrix)
   return A.base_ring
end

function elem_type(S::MatSpace)
   return typeof(zero(S))
end

elem_type(::Type{MatSpace{T}}) where T <: NCRingElement = GenericMatrix{T}
parent_type(::Type{<:GenericMatrix{T}}) where T <: NCRingElement = MatSpace{T}
base_ring_type(::Type{MatSpace{T}}) where T <: NCRingElement = parent_type(T)
base_ring_type(::Type{<:GenericMatrix{T}}) where T <: NCRingElement = parent_type(T)
eltype(::Type{MatSpace{T}}) where T <: NCRingElement = GenericMatrix{T}
eltype(S::MatSpace{T}) where T <: NCRingElement = GenericMatrix{T}

function parent_type(A::GenericMatrix)
   return typeof(parent(A))
end

number_of_rows(S::MatSpace) = S.nrows
number_of_columns(S::MatSpace) = S.ncols
number_of_rows(A::GenericMatrix) = A.nrows
number_of_columns(A::GenericMatrix) = A.ncols

size(A::GenericMatrix) = (number_of_rows(A), number_of_columns(A))
size(A::GenericMatrix, d::Integer) = d <= 2 ? size(A)[d] : 1
length(A::GenericMatrix) = number_of_rows(A) * number_of_columns(A)

function getindex(A::GenericMatrix, i::Int, j::Int)
   return _matrix_coerce(base_ring(A), A.entries[_matrix_index(A, i, j)])
end

function setindex!(A::GenericMatrix, value, i::Int, j::Int)
   A.entries[_matrix_index(A, i, j)] = _matrix_coerce(base_ring(A), value)
   return A
end

function check_parent(A::MatrixElem, B::MatrixElem, throw::Bool = true)
   flag = base_ring(A) === base_ring(B) && number_of_rows(A) == number_of_rows(B) && number_of_columns(A) == number_of_columns(B)
   flag || !throw || error("Incompatible matrix spaces in matrix operation")
   return flag
end

function zero_matrix(R::NCRing, r::Int, c::Int)
   entries = Any[]
   z = zero(R)
   i = 1
   while i <= r * c
      push!(entries, z)
      i += 1
   end
   return _matrix_from_flat(R, r, c, entries)
end

function identity_matrix(R::NCRing, n::Int)
   A = zero_matrix(R, n, n)
   i = 1
   while i <= n
      A[i, i] = one(R)
      i += 1
   end
   return A
end

zero(S::MatSpace) = zero_matrix(base_ring(S), number_of_rows(S), number_of_columns(S))
one(S::MatSpace) = identity_matrix(base_ring(S), number_of_rows(S))

function iszero(A::GenericMatrix)
   i = 1
   while i <= number_of_rows(A)
      j = 1
      while j <= number_of_columns(A)
         iszero(A[i, j]) || return false
         j += 1
      end
      i += 1
   end
   return true
end

function isone(A::GenericMatrix)
   number_of_rows(A) == number_of_columns(A) || return false
   i = 1
   while i <= number_of_rows(A)
      j = 1
      while j <= number_of_columns(A)
         if i == j
            isone(A[i, j]) || return false
         else
            iszero(A[i, j]) || return false
         end
         j += 1
      end
      i += 1
   end
   return true
end

function ==(A::GenericMatrix, B::GenericMatrix)
   check_parent(A, B)
   i = 1
   while i <= number_of_rows(A)
      j = 1
      while j <= number_of_columns(A)
         A[i, j] == B[i, j] || return false
         j += 1
      end
      i += 1
   end
   return true
end

function -(A::GenericMatrix)
   out = Any[]
   i = 1
   while i <= number_of_rows(A)
      j = 1
      while j <= number_of_columns(A)
         push!(out, -A[i, j])
         j += 1
      end
      i += 1
   end
   return _matrix_from_flat(base_ring(A), number_of_rows(A), number_of_columns(A), out)
end

function +(A::GenericMatrix, B::GenericMatrix)
   check_parent(A, B)
   out = Any[]
   i = 1
   while i <= number_of_rows(A)
      j = 1
      while j <= number_of_columns(A)
         push!(out, A[i, j] + B[i, j])
         j += 1
      end
      i += 1
   end
   return _matrix_from_flat(base_ring(A), number_of_rows(A), number_of_columns(A), out)
end

function -(A::GenericMatrix, B::GenericMatrix)
   return A + (-B)
end

function *(A::GenericMatrix, B::GenericMatrix)
   number_of_columns(A) == number_of_rows(B) || error("Incompatible matrix dimensions")
   base_ring(A) === base_ring(B) || error("Base rings do not match")
   R = base_ring(A)
   out = Any[]
   i = 1
   while i <= number_of_rows(A)
      j = 1
      while j <= number_of_columns(B)
         s = zero(R)
         k = 1
         while k <= number_of_columns(A)
            s = s + A[i, k] * B[k, j]
            k += 1
         end
         push!(out, s)
         j += 1
      end
      i += 1
   end
   return _matrix_from_flat(R, number_of_rows(A), number_of_columns(B), out)
end

function *(A::GenericMatrix, c::NCRingElement)
   out = Any[]
   i = 1
   while i <= number_of_rows(A)
      j = 1
      while j <= number_of_columns(A)
         push!(out, A[i, j] * c)
         j += 1
      end
      i += 1
   end
   return _matrix_from_flat(base_ring(A), number_of_rows(A), number_of_columns(A), out)
end

function *(c::NCRingElement, A::GenericMatrix)
   return A * c
end

function transpose(A::GenericMatrix)
   out = Any[]
   j = 1
   while j <= number_of_columns(A)
      i = 1
      while i <= number_of_rows(A)
         push!(out, A[i, j])
         i += 1
      end
      j += 1
   end
   return _matrix_from_flat(base_ring(A), number_of_columns(A), number_of_rows(A), out)
end

function tr(A::GenericMatrix)
   number_of_rows(A) == number_of_columns(A) || error("Not a square matrix in trace")
   s = zero(base_ring(A))
   i = 1
   while i <= number_of_rows(A)
      s = s + A[i, i]
      i += 1
   end
   return s
end

function _matrix_minor(A::GenericMatrix, row::Int, col::Int)
   out = Any[]
   i = 1
   while i <= number_of_rows(A)
      if i != row
         j = 1
         while j <= number_of_columns(A)
            if j != col
               push!(out, A[i, j])
            end
            j += 1
         end
      end
      i += 1
   end
   return _matrix_from_flat(base_ring(A), number_of_rows(A) - 1, number_of_columns(A) - 1, out)
end

function det(A::GenericMatrix)
   number_of_rows(A) == number_of_columns(A) || error("Not a square matrix in det")
   n = number_of_rows(A)
   if n == 0
      return one(base_ring(A))
   elseif n == 1
      return A[1, 1]
   elseif n == 2
      return A[1, 1] * A[2, 2] - A[1, 2] * A[2, 1]
   end
   d = zero(base_ring(A))
   j = 1
   while j <= n
      term = A[1, j] * det(_matrix_minor(A, 1, j))
      if isodd(j)
         d = d + term
      else
         d = d - term
      end
      j += 1
   end
   return d
end

function rank(A::GenericMatrix)
   if number_of_rows(A) == 0 || number_of_columns(A) == 0
      return 0
   elseif number_of_rows(A) == 1 && number_of_columns(A) == 1
      return iszero(A[1, 1]) ? 0 : 1
   elseif number_of_rows(A) == 2 && number_of_columns(A) == 2
      !iszero(det(A)) && return 2
      i = 1
      while i <= 2
         j = 1
         while j <= 2
            !iszero(A[i, j]) && return 1
            j += 1
         end
         i += 1
      end
      return 0
   end
   error("rank is only supported for empty, 1x1, and 2x2 matrices in the MVP")
end

function _matrix_base_name(R::Integers{BigInt})
   return "Integers"
end

function _matrix_base_name(R::Rationals{BigInt})
   return "Rationals"
end

function _matrix_base_name(R::GenericPolyRing)
   return _poly_ring_to_string(R)
end

function _matrix_base_name(R)
   return string(R)
end

function _matrix_space_to_string(S::MatSpace)
   return "Matrix space of " * string(number_of_rows(S)) * " rows and " *
          string(number_of_columns(S)) * " columns over " * _matrix_base_name(base_ring(S))
end

function _matrix_to_string(A::GenericMatrix)
   out = ""
   i = 1
   while i <= number_of_rows(A)
      row = "["
      j = 1
      while j <= number_of_columns(A)
         j > 1 && (row *= "   ")
         row *= string(A[i, j])
         j += 1
      end
      row *= "]"
      out *= row
      i < number_of_rows(A) && (out *= "\n")
      i += 1
   end
   return out
end

function show(io::IO, S::MatSpace)
   print(io, _matrix_space_to_string(S))
end

function show(io::IO, A::GenericMatrix)
   print(io, _matrix_to_string(A))
end
