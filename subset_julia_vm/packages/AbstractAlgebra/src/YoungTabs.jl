###############################################################################
#
#   Partitions and Young tableaux MVP
#
###############################################################################

struct Partition <: AbstractVector
   n::Int
   part::Vector

   function Partition(n::Integer, part::Vector)
      return new(Int(n), part)
   end
end

struct YoungTableau <: AbstractMatrix
   part::Partition
   fill::Vector

   function YoungTableau(part::Partition, fill::Vector)
      return new(part, fill)
   end
end

function _partition_normalize(part, check::Bool=true)
   out = Int[]
   i = 1
   while i <= length(part)
      push!(out, Int(part[i]))
      i += 1
   end
   if check
      i = 1
      while i <= length(out)
         out[i] > 0 || error("Found non-positive entry in partition")
         i += 1
      end
      i = 1
      while i <= length(out)
         j = i + 1
         while j <= length(out)
            if out[j] > out[i]
               tmp = out[i]
               out[i] = out[j]
               out[j] = tmp
            end
            j += 1
         end
         i += 1
      end
   end
   return out
end

function Partition(part, check::Bool=true)
   out = _partition_normalize(part, check)
   n = 0
   i = 1
   while i <= length(out)
      n += out[i]
      i += 1
   end
   return Partition(n, out)
end

function Partition(n::Integer, part, check::Bool)
   p = Partition(part, check)
   p.n == Int(n) || error("partition sum does not match")
   return p
end

size(p::Partition) = size(p.part)

function length(p::Partition)
   return size(p.part)[1]
end
getindex(p::Partition, i::Integer) = p.part[Int(i)]
sum(p::Partition) = p.n

function ==(p::Partition, q::Partition)
   p.n == q.n || return false
   return p.part == q.part
end

function _partition_to_string(p::Partition)
   return string(p.part)
end

function _young_diagram_to_string(p::Partition)
   rows = String[]
   i = 1
   while i <= length(p.part)
      push!(rows, repeat("#", p.part[i]))
      i += 1
   end
   return join(rows, "\n")
end

function show(io::IO, p::Partition)
   print(io, _partition_to_string(p))
end

function _young_fill_default(p::Partition)
   out = Int[]
   i = 1
   while i <= p.n
      push!(out, i)
      i += 1
   end
   return out
end

function YoungTableau(p::Partition, fill=_young_fill_default(p))
   length(fill) == p.n || error("Length of fill vector must match the size of partition")
   out = Int[]
   i = 1
   while i <= length(fill)
      push!(out, Int(fill[i]))
      i += 1
   end
   return YoungTableau(p, out)
end

function YoungTableau(part, fill=nothing)
   p = Partition(part)
   if fill === nothing
      return YoungTableau(p)
   end
   return YoungTableau(p, fill)
end

function size(Y::YoungTableau)
   return (size(Y.part)[1], Y.part[1])
end

function _young_in_shape(Y::YoungTableau, i::Int, j::Int)
   i > 0 || return false
   j > 0 || return false
   i <= size(Y.part)[1] || return false
   return j <= Y.part[i]
end

function getindex(Y::YoungTableau, n::Integer)
   idx = Int(n)
   idx < 1 && error("BoundsError")
   r, c = size(Y)
   i = ((idx - 1) % r) + 1
   j = div(idx - 1, r) + 1
   if !_young_in_shape(Y, i, j)
      return 0
   end
   k = 0
   row = 1
   while row < i
      k += Y.part[row]
      row += 1
   end
   k += j
   return Y.fill[k]
end

function ==(Y::YoungTableau, Z::YoungTableau)
   Y.part == Z.part || return false
   return Y.fill == Z.fill
end

function _young_tableau_to_string(Y::YoungTableau)
   rows = String[]
   k = 1
   i = 1
   while i <= size(Y.part)[1]
      cells = String[]
      j = 1
      while j <= Y.part[i]
         push!(cells, string(Y.fill[k]))
         k += 1
         j += 1
      end
      push!(rows, join(cells, " "))
      i += 1
   end
   return join(rows, "\n")
end

function show(io::IO, Y::YoungTableau)
   print(io, _young_tableau_to_string(Y))
end
