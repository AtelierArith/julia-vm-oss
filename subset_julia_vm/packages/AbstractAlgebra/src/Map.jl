###############################################################################
#
#   Map MVP
#
###############################################################################

struct SimpleMap{D, C, F} <: Map{D, C, FunctionalMap, Any}
   domain::D
   codomain::C
   func::F
end

struct SimpleIdentityMap{D} <: Map{D, D, IdentityMap, Any}
   domain::D
end

function domain(f::SimpleMap)
   return f.domain
end

function codomain(f::SimpleMap)
   return f.codomain
end

function domain(f::SimpleIdentityMap)
   return f.domain
end

function codomain(f::SimpleIdentityMap)
   return f.domain
end

function hom(D, C, f)
   return SimpleMap{typeof(D), typeof(C), typeof(f)}(D, C, f)
end

function identity_map(D)
   return SimpleIdentityMap{typeof(D)}(D)
end

function (f::SimpleMap)(x)
   return f.func(x)
end

function (f::SimpleIdentityMap)(x)
   return x
end

function _map_to_string(f::SimpleMap)
   return "Map from " * string(domain(f)) * " to " * string(codomain(f))
end

function _map_to_string(f::SimpleIdentityMap)
   return "Identity map on " * string(domain(f))
end

function show(io::IO, f::SimpleMap)
   print(io, _map_to_string(f))
end

function show(io::IO, f::SimpleIdentityMap)
   print(io, _map_to_string(f))
end
