###############################################################################
#
#   Permutation group MVP
#
###############################################################################

struct CycleDec{T <: Integer}
   cycles::Vector
end

# Workaround: keep the internal element type distinct from the exported `Perm`
# constructor until imported parametric inner constructors are callable by bare
# exported name. (Issue #8313)
mutable struct AAPerm <: AbstractPerm
   d::Vector
end

struct SymmetricGroup{T <: Integer} <: AbstractPermutationGroup
   n::T
end

function SymmetricGroup(n::T) where T <: Integer
   n < 0 && error("SymmetricGroup constructor requires a non-negative integer")
   return SymmetricGroup{T}(n)
end

function Perm(n::T) where T <: Integer
   n < 0 && error("Perm constructor requires a non-negative integer")
   out = Int[]
   i = 1
   while i <= n
      push!(out, Int(i))
      i += 1
   end
   return AAPerm(out)
end

function _validate_perm(v::Vector)
   n = length(v)
   seen = [false for _ in 1:n]
   i = 1
   while i <= n
      x = Int(v[i])
      if x < 1 || x > n || seen[x]
         error("Unable to coerce to permutation: non-unique elements in array")
      end
      seen[x] = true
      i += 1
   end
   return true
end

function Perm(d, check::Bool = true)
   return AAPerm(d)
end

function perm(a::Vector{T}, check::Bool = true) where T <: Integer
   return Perm(a, check)
end

parent_type(::Type{AAPerm}) = SymmetricGroup{Int}
parent_type(g::AAPerm) = typeof(parent(g))
parent(g::AAPerm) = SymmetricGroup(length(g.d))
elem_type(::Type{SymmetricGroup{T}}) where T <: Integer = AAPerm
elem_type(G::SymmetricGroup{T}) where T <: Integer = AAPerm

function check_parent(g::AAPerm, h::AAPerm)
   length(g.d) == length(h.d) || error("incompatible permutation groups")
   return true
end

function ==(g::AAPerm, h::AAPerm)
   length(g.d) == length(h.d) || return false
   i = 1
   while i <= length(g.d)
      g.d[i] == h.d[i] || return false
      i += 1
   end
   return true
end

function ==(G::SymmetricGroup, H::SymmetricGroup)
   return typeof(G) == typeof(H) && G.n == H.n
end

length(g::AAPerm) = length(g.d)
length(G::SymmetricGroup) = Int(factorial(Int(G.n)))

function getindex(g::AAPerm, n::Integer)
   return g.d[Int(n)]
end

function setindex!(g::AAPerm, v::Integer, n::Integer)
   g.d[Int(n)] = Int(v)
   return g
end

function similar(g::AAPerm)
   return Perm(length(g.d))
end

one(G::SymmetricGroup) = Perm(G.n)
one(g::AAPerm) = one(parent(g))

function isone(g::AAPerm)
   i = 1
   while i <= length(g.d)
      g.d[i] == i || return false
      i += 1
   end
   return true
end

function *(g::AAPerm, h::AAPerm)
   check_parent(g, h)
   out = Int[]
   i = 1
   while i <= length(g.d)
      push!(out, h.d[Int(g.d[i])])
      i += 1
   end
   return Perm(out, false)
end

function inv(g::AAPerm)
   res = Perm(length(g.d))
   i = 1
   while i <= length(g.d)
      res.d[Int(g.d[i])] = Int(i)
      i += 1
   end
   return res
end

function ^(g::AAPerm, n::Integer)
   if n < 0
      return inv(g)^(-n)
   end
   result = one(g)
   base = g
   e = Int(n)
   while e > 0
      if isodd(e)
         result = result * base
      end
      e = div(e, 2)
      if e > 0
         base = base * base
      end
   end
   return result
end

function _perm_cycles(g::AAPerm)
   n = length(g.d)
   visited = [false for _ in 1:n]
   out = Any[]
   i = 1
   while i <= n
      if !visited[i]
         cyc = Int[]
         j = i
         while !visited[j]
            push!(cyc, j)
            visited[j] = true
            j = Int(g.d[j])
         end
         push!(out, cyc)
      end
      i += 1
   end
   return out
end

function cycles(g::AAPerm)
   return CycleDec{Int}(_perm_cycles(g))
end

function permtype(g::AAPerm)
   lengths = Int[]
   cs = _perm_cycles(g)
   i = 1
   while i <= length(cs)
      push!(lengths, length(cs[i]))
      i += 1
   end
   i = 1
   while i <= length(lengths)
      j = i + 1
      while j <= length(lengths)
         if lengths[j] > lengths[i]
            tmp = lengths[i]
            lengths[i] = lengths[j]
            lengths[j] = tmp
         end
         j += 1
      end
      i += 1
   end
   return lengths
end

function parity(g::AAPerm)
   p = 0
   cs = _perm_cycles(g)
   i = 1
   while i <= length(cs)
      p = (p + length(cs[i]) - 1) % 2
      i += 1
   end
   return p
end

function sign(g::AAPerm)
   return parity(g) == 0 ? 1 : -1
end

function _cycle_to_string(c)
   s = "("
   i = 1
   while i <= length(c)
      i > 1 && (s *= ",")
      s *= string(c[i])
      i += 1
   end
   s *= ")"
   return s
end

function _perm_to_string(g::AAPerm)
   isone(g) && return "()"
   s = ""
   cs = _perm_cycles(g)
   i = 1
   while i <= length(cs)
      if length(cs[i]) > 1
         s *= _cycle_to_string(cs[i])
      end
      i += 1
   end
   return isempty(s) ? "()" : s
end

function _symmetric_group_to_string(G::SymmetricGroup)
   return "Full symmetric group over " * string(G.n) * " elements"
end

function show(io::IO, g::AAPerm)
   print(io, _perm_to_string(g))
end

function show(io::IO, G::SymmetricGroup)
   print(io, _symmetric_group_to_string(G))
end

function setpermstyle(format::Symbol)
   if format == :array || format == :cycles
      return format
   end
   error("Permutations can be displayed only as :array or :cycles.")
end

function number_of_generators(G::SymmetricGroup)
   return G.n == 1 ? 0 : G.n == 2 ? 1 : 2
end

function gens(G::SymmetricGroup)
   if G.n == 1
      return Any[]
   elseif G.n == 2
      return Any[Perm([2, 1])]
   end
   a = Perm(Int(G.n))
   i = 1
   while i < G.n
      a[i] = i + 1
      i += 1
   end
   a[Int(G.n)] = 1
   b = Perm(Int(G.n))
   b[1] = 2
   b[2] = 1
   return Any[a, b]
end

function gen(G::SymmetricGroup, i::Int)
   return gens(G)[i]
end
