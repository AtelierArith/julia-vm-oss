###############################################################################
#
#   Core AbstractAlgebra interface
#
###############################################################################

function elem_type end
elem_type(x) = elem_type(typeof(x))
elem_type(T::DataType) = throw(MethodError(elem_type, (T,)))
elem_type(T::Type{Union{}}) = throw(MethodError(elem_type, (T,)))

function parent_type end
parent_type(x) = parent_type(typeof(x))
parent_type(T::DataType) = throw(MethodError(parent_type, (T,)))
parent_type(T::Type{Union{}}) = throw(MethodError(parent_type, (T,)))

function base_ring end
base_ring(x::ModuleElem) = base_ring(parent(x))
base_ring(x::NCRingElement) = base_ring(parent(x))

function base_ring_type end
base_ring_type(x) = base_ring_type(typeof(x))
base_ring_type(x::Type{<:NCRingElement}) = base_ring_type(parent_type(x))
base_ring_type(x::Type{<:ModuleElem}) = base_ring_type(parent_type(x))
base_ring_type(x::Type{<:Ideal}) = base_ring_type(parent_type(x))
base_ring_type(T::DataType) = throw(MethodError(base_ring_type, (T,)))
base_ring_type(T::Type{Union{}}) = throw(MethodError(base_ring_type, (T,)))

function coefficient_ring end
coefficient_ring(x::NCRingElement) = coefficient_ring(parent(x))

function coefficient_ring_type end
coefficient_ring_type(x) = coefficient_ring_type(typeof(x))
coefficient_ring_type(x::Type{<:NCRingElement}) = coefficient_ring_type(parent_type(x))
coefficient_ring_type(x::Type{<:ModuleElem}) = coefficient_ring_type(parent_type(x))
coefficient_ring_type(x::Type{<:Ideal}) = coefficient_ring_type(parent_type(x))
coefficient_ring_type(T::DataType) = throw(MethodError(coefficient_ring_type, (T,)))
coefficient_ring_type(T::Type{Union{}}) = throw(MethodError(coefficient_ring_type, (T,)))

function is_exact_type end
function is_domain_type end
function characteristic end
function check_parent end
function check_base_ring end

function divexact end
function divides end
function is_divisible_by end
function is_unit end
function is_zero_divisor end
function canonical_unit end
function expressify end
function sqrt end
function is_square end
function is_square_with_sqrt end
function root end
function gcdinv end
function fraction_field end
function residue_ring end
function modulus end
function data end
function lift end
function matrix end
function matrix_space end
function zero_matrix end
function identity_matrix end
function free_module end
function domain end
function codomain end
function identity_map end
function hom end

zero!(a) = zero(parent(a))
one!(a) = one(parent(a))

function check_parent(a, b, throw::Bool = true)
   flag = parent(a) === parent(b)
   flag || !throw || error("parents do not match")
   return flag
end

function check_base_ring(a, b, throw::Bool = true)
   flag = base_ring(a) === base_ring(b)
   flag || !throw || error("base rings do not match")
   return flag
end
