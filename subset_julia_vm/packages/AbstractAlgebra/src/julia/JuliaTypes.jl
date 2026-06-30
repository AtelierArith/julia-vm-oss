###############################################################################
#
#   Julia parent objects
#
###############################################################################

struct Integers{T <: Integer} <: Ring
end

struct Rationals{T <: Integer} <: Field
end

struct Floats{T <: AbstractFloat} <: Field
end
