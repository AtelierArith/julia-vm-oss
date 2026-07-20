# vm_aot equivalence corpus widening (Issue #10815): Bool/comparison/unary
# operators exercised through the AoT minimal-prelude codegen path
# (`subset_julia_vm/src/julia/internal/prelude_aot.jl`). Locks VM output and
# widens the differential lane beyond the 3 ADR #8639 acceptance kernels.
a::Bool = true
b::Bool = false
println(a & b)
println(a | b)
println(!a)
println(xor(a, b))
x::Int64 = 5
y::Int64 = -5
println(-x)
println(x == 5)
println(x != y)
println(x >= y)
println(x <= y)

(a & b) == false && (a | b) == true && (!a) == false && xor(a, b) == true &&
    (-x) == -5 && (x == 5) && (x != y) && (x >= y) && (x <= y) == false
