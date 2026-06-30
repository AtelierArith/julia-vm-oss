# Issue #8092: a concrete parametric struct must not be `isa`/`<:` an unrelated
# sibling parametric struct that shares a parametric abstract supertype. The
# context-free typevar heuristic used to misclassify short uppercase[+digit]
# struct names (`A2`, `P`) as free type variables, so `RM <: A2` reduced to
# `X <: free-typevar` (true) and `A2 <: Rot` to `free-typevar <: Named` (false).
abstract type Rot8092{N,T} end
struct RM8092{N,T} <: Rot8092{N,T}
    x::T
end
struct A28092{T} <: Rot8092{2,T}
    y::T
end

r = RM8092{2,Float64}(1.0)
a = A28092{Float64}(2.0)

ok = (RM8092 <: A28092) == false &&
     (A28092 <: RM8092) == false &&
     (RM8092 <: Rot8092) == true &&
     (A28092 <: Rot8092) == true &&
     (RM8092{2,Float64} <: A28092{Float64}) == false &&
     (r isa A28092) == false &&
     (r isa RM8092) == true &&
     (a isa RM8092) == false &&
     (a isa A28092) == true &&
     (r isa Rot8092) == true &&
     (a isa Rot8092) == true

println(ok)
ok
