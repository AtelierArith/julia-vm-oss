###############################################################################
#
#   Mat space
#
###############################################################################

struct MatSpace{T <: NCRingElement} <: Module{T}
  base_ring::NCRing
  nrows::Int
  ncols::Int

  function MatSpace{T}(R::NCRing, r::Int, c::Int, cached::Bool = true) where T <: NCRingElement
     (r < 0 || c < 0) && error("Dimensions must be non-negative")
     return new{T}(R, r, c)
  end
end

###############################################################################
#
#   Universal ring
#
###############################################################################

@attributes mutable struct UniversalRing{T <: RingElem, U <: RingElement} <: Ring
  base_ring::Ring

  function UniversalRing(R::Ring)
    # Workaround: upstream computes these type parameters with
    # `elem_type(R)` / `elem_type(coefficient_ring(R))`, but dynamic `new{...}`
    # inner constructor parameters are not supported yet. (Issue #7935)
    return new{RingElem, RingElem}(R)
  end
end

mutable struct UniversalRingElem{T <: RingElem, U <: RingElement} <: RingElem
  data::T
  parent::UniversalRing{T, U}
end

###############################################################################
#
#   Universal polynomial ring
#
###############################################################################

struct UnivPolyCoeffs{T <: RingElem}
  poly::T
end

struct UnivPolyExponentVectors{T <: RingElem}
  poly::T
end

struct UnivPolyTerms{T <: RingElem}
  poly::T
end

struct UnivPolyMonomials{T <: RingElem}
  poly::T
end
