# Issue #9126: F64 accumulate loop bodies fuse to AddF64Slots / AddF64I64Slots.
# The results must match upstream Julia exactly; this fixture is the runtime
# oracle for the fused superinstructions (the peephole-shape assertions live in
# subset_julia_vm_bytecode::peephole unit tests).

# `s += Float64(i)` — AddF64I64Slots (I64 slot converted to F64).
function accum_i64_to_f64(n::Int64)
    s = 0.0
    for i in 1:n
        s = s + Float64(i)
    end
    return s
end

# `s += step` — AddF64Slots (both operands F64 slots).
function accum_f64(n::Int64, step::Float64)
    s = 0.0
    for i in 1:n
        s = s + step
    end
    return s
end

# 3-slot add where dst != lhs (general AddF64Slots form).
function three_slot(a::Float64, b::Float64, n::Int64)
    c = 0.0
    for i in 1:n
        c = a + b
    end
    return c
end

println(accum_i64_to_f64(100))
println(accum_f64(1000, 0.25))
println(three_slot(1.5, 2.5, 10))
"5050.0\n250.0\n4.0\n"
