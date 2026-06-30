module M8313
mutable struct Perm{T <: Integer}
   d::Vector{T}
   function Perm(d)
      return new{Int}(d)
   end
end
export Perm
end

using .M8313
p = Perm([1, 2, 3])
length(p.d) == 3
