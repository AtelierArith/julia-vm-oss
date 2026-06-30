###############################################################################
#
#   Free module MVP
#
###############################################################################

struct FreeModule{T <: NCRingElement, R <: NCRing} <: Module{T}
   base_ring::R
   rank::Int
end

struct FreeModuleElem{T <: NCRingElement, R <: NCRing} <: ModuleElem{T}
   coords::Vector
   parent::FreeModule{T, R}
end

function free_module(R::NCRing, n::Int)
   n < 0 && error("Rank must be non-negative")
   return FreeModule{elem_type(R), typeof(R)}(R, n)
end

function parent(v::FreeModuleElem)
   return v.parent
end

function base_ring(M::FreeModule)
   return M.base_ring
end

function base_ring(v::FreeModuleElem)
   return base_ring(parent(v))
end

number_of_generators(M::FreeModule) = M.rank

elem_type(::Type{FreeModule{T, R}}) where {T <: NCRingElement, R <: NCRing} = FreeModuleElem{T, R}
parent_type(::Type{FreeModuleElem{T, R}}) where {T <: NCRingElement, R <: NCRing} = FreeModule{T, R}
base_ring_type(::Type{FreeModule{T, R}}) where {T <: NCRingElement, R <: NCRing} = R

function elem_type(M::FreeModule)
   return typeof(zero(M))
end

function parent_type(v::FreeModuleElem)
   return typeof(parent(v))
end

function (M::FreeModule)(coords::Vector)
   length(coords) == M.rank || error("coordinates have wrong length")
   out = Any[]
   i = 1
   while i <= length(coords)
      push!(out, _matrix_coerce(base_ring(M), coords[i]))
      i += 1
   end
   return FreeModuleElem{elem_type(base_ring(M)), typeof(base_ring(M))}(out, M)
end

function zero(M::FreeModule)
   out = Any[]
   i = 1
   while i <= M.rank
      push!(out, zero(base_ring(M)))
      i += 1
   end
   return M(out)
end

function gen(M::FreeModule, i::Int)
   1 <= i <= M.rank || error("generator index out of bounds")
   out = Any[]
   j = 1
   while j <= M.rank
      push!(out, i == j ? one(base_ring(M)) : zero(base_ring(M)))
      j += 1
   end
   return M(out)
end

function gens(M::FreeModule)
   out = Any[]
   i = 1
   while i <= M.rank
      push!(out, gen(M, i))
      i += 1
   end
   return out
end

function getindex(v::FreeModuleElem, i::Int)
   1 <= i <= length(v.coords) || error("module coordinate index out of bounds")
   return _matrix_coerce(base_ring(v), v.coords[i])
end

function ==(a::FreeModuleElem, b::FreeModuleElem)
   check_parent(a, b)
   i = 1
   while i <= length(a.coords)
      a[i] == b[i] || return false
      i += 1
   end
   return true
end

function +(a::FreeModuleElem, b::FreeModuleElem)
   check_parent(a, b)
   out = Any[]
   i = 1
   while i <= length(a.coords)
      push!(out, a[i] + b[i])
      i += 1
   end
   return parent(a)(out)
end

function -(a::FreeModuleElem)
   out = Any[]
   i = 1
   while i <= length(a.coords)
      push!(out, -a[i])
      i += 1
   end
   return parent(a)(out)
end

function -(a::FreeModuleElem, b::FreeModuleElem)
   return a + (-b)
end

function _free_module_scalar_mul(c, v::FreeModuleElem)
   out = Any[]
   i = 1
   while i <= length(v.coords)
      push!(out, _matrix_coerce(base_ring(v), c) * v[i])
      i += 1
   end
   return parent(v)(out)
end

*(c::NCRingElement, v::FreeModuleElem) = _free_module_scalar_mul(c, v)
*(c::Integer, v::FreeModuleElem) = _free_module_scalar_mul(c, v)
*(c::Rational, v::FreeModuleElem) = _free_module_scalar_mul(c, v)

function _free_module_to_string(M::FreeModule)
   return "Free module of rank " * string(M.rank) * " over " * _matrix_base_name(base_ring(M))
end

function _free_module_elem_to_string(v::FreeModuleElem)
   out = "("
   i = 1
   while i <= length(v.coords)
      i > 1 && (out *= ", ")
      out *= string(v[i])
      i += 1
   end
   return out * ")"
end

function show(io::IO, M::FreeModule)
   print(io, _free_module_to_string(M))
end

function show(io::IO, v::FreeModuleElem)
   print(io, _free_module_elem_to_string(v))
end
