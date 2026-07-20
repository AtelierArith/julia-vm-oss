# Fixture: Memory{T} lattice tracking (Issue #9009)
#
# Before this fix, Memory{T} had no ConcreteType lattice variant and no
# ValueType mapping in julia_type_to_value_type_with_table.  The effect was:
#   * `m::Memory{Int64}` parameters showed as `m::Any` in the FunctionInfo
#     metadata (wrong method-table signature)
#   * The abstract interpreter saw Memory values as Any, losing element-type
#     precision across function call boundaries
#
# This fixture verifies that:
#   1. Memory{T} parameter annotations compile and dispatch correctly
#   2. Operations on Memory{T} parameters return the right element type
#   3. Multiple element types (Int64, Float64) are distinguished

function sum_mem(m::Memory{Int64})
    s = 0
    for i in 1:length(m)
        s += m[i]
    end
    return s
end

function fill_mem!(m::Memory{Float64}, v::Float64)
    for i in 1:length(m)
        m[i] = v
    end
end

function first_elem_i64(m::Memory{Int64})
    return m[1]
end

function first_elem_f64(m::Memory{Float64})
    return m[1]
end

m = Memory{Int64}(undef, 3)
m[1] = 10
m[2] = 20
m[3] = 30
println(sum_mem(m))
println(first_elem_i64(m))

mf = Memory{Float64}(undef, 2)
fill_mem!(mf, 3.14)
println(mf[1])
println(first_elem_f64(mf))
println(mf[2])

# Bare Memory (unknown element type) also works
mb = Memory{UInt8}(undef, 4)
mb[1] = 0xff
println(Int(mb[1]))

true
