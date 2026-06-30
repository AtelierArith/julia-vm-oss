###############################################################################
#
#   Abstract types
#
###############################################################################

# broad mathematical domains
abstract type Set end
abstract type Group <: Set end
abstract type AdditiveGroup <: Set end
abstract type AbstractPermutationGroup <: Group end
abstract type NCRing <: Set end
abstract type Ring <: NCRing end
abstract type Field <: Ring end

# elements of mathematical domains
abstract type SetElem end
abstract type GroupElem <: SetElem end
abstract type AdditiveGroupElem <: SetElem end
abstract type AbstractPerm <: GroupElem end
abstract type NCRingElem <: SetElem end
abstract type RingElem <: NCRingElem end
abstract type FieldElem <: RingElem end

# unions of AbstractAlgebra abstract types and Julia types
const JuliaRingElement = Union{Integer, Rational, AbstractFloat}
const JuliaFieldElement = Union{Rational, AbstractFloat}
const JuliaExactRingElement = Union{Integer, Rational}

const RingElement = Union{RingElem, JuliaRingElement}
const NCRingElement = Union{NCRingElem, JuliaRingElement}
const FieldElement = Union{FieldElem, JuliaFieldElement}

# parameterized domains
abstract type Module{T<:NCRingElement} <: AdditiveGroup end
abstract type FPModule{T} <: Module{T} end
abstract type Ideal{T} <: Set end
abstract type IdealSet{T} <: Set end

# elements of parameterised domains
abstract type ModuleElem{T<:NCRingElement} <: AdditiveGroupElem end
abstract type FPModuleElem{T} <: ModuleElem{T} end
abstract type IdealElem{T} <: SetElem end
abstract type Map{D, C, S, T} <: SetElem end
abstract type SetMap end
abstract type FunctionalMap <: SetMap end
abstract type IdentityMap <: SetMap end
abstract type FPModuleHomomorphism <: FunctionalMap end

# rings, fields etc, parameterised by an element type
abstract type PolyRing{T<:RingElement} <: Ring end
abstract type NCPolyRing{T<:NCRingElement} <: NCRing end
abstract type MPolyRing{T<:RingElement} <: Ring end
abstract type LaurentPolyRing{T<:RingElement} <: Ring end
abstract type LaurentMPolyRing{T<:RingElement} <: Ring end
abstract type SeriesRing{T<:RingElement} <: Ring end
abstract type MSeriesRing{T<:RingElement} <: Ring end
abstract type ResidueRing{T<:RingElement} <: Ring end
abstract type ResidueField{T<:RingElement} <: Field end
abstract type FracField{T<:RingElement} <: Field end
abstract type TotFracRing{T<:RingElement} <: Ring end
abstract type MatRing{T<:NCRingElement} <: NCRing end
abstract type FreeAssociativeAlgebra{T<:RingElement} <: NCRing end
abstract type NumField{T<:RingElement} <: Field end
abstract type SimpleNumField{T} <: NumField{T} end

# mathematical objects parameterised by an element type
abstract type PolyRingElem{T<:RingElement} <: RingElem end
abstract type NCPolyRingElem{T<:NCRingElement} <: NCRingElem end
abstract type MPolyRingElem{T<:RingElement} <: RingElem end
abstract type LaurentPolyRingElem{T<:RingElement} <: RingElem end
abstract type LaurentMPolyRingElem{T<:RingElement} <: RingElem end
abstract type ResElem{T<:RingElement} <: RingElem end
abstract type ResFieldElem{T<:RingElement} <: FieldElem end
abstract type FracElem{T<:RingElement} <: FieldElem end
abstract type TotFrac{T<:RingElement} <: RingElem end
abstract type SeriesElem{T<:RingElement} <: RingElem end
abstract type MSeriesElem{T<:RingElement} <: RingElem end
abstract type RelPowerSeriesRingElem{T} <: SeriesElem{T} end
abstract type AbsPowerSeriesRingElem{T} <: SeriesElem{T} end
abstract type AbsMSeriesElem{T} <: MSeriesElem{T} end
abstract type MatElem{T} <: ModuleElem{T} end
abstract type MatRingElem{T<:NCRingElement} <: NCRingElem end
abstract type FreeAssociativeAlgebraElem{T<:RingElement} <: NCRingElem end
abstract type NumFieldElem{T<:RingElement} <: FieldElem end
abstract type SimpleNumFieldElem{T} <: NumFieldElem{T} end

# additional abstract types for parents and elements
abstract type FinField <: Field end
abstract type FinFieldElem <: FieldElem end

################################################################################
#
#   Promotion system and early generic declarations
#
################################################################################

promote_rule(T, U) = Union{}
promote_rule(a::Type{S}, b::Type{T}) where {S <: Real, T <: Real} = Base.promote_rule(a, b)

function elem_type end
function parent_type end
function base_ring end
function coefficient_ring end
function symbols end
