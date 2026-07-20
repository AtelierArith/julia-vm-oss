mutable struct EquivWeakBox10092
    x::Int
end

function make_equiv_weak_ref_10092()
    tmp = EquivWeakBox10092(7)
    return WeakRef(tmp)
end

wr = make_equiv_weak_ref_10092()
GC.gc()
GC.gc()
println(typeof(wr))
println(wr.value === nothing)

rooted = EquivWeakBox10092(3)
wr2 = WeakRef(rooted)
GC.gc()
println(wr2.value === rooted)
println(wr2.value.x)
