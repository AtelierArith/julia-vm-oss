# Issue #9123: loop-carried Union{Int64,Float64} values must not degrade to
# fully dynamic dispatch.  A function whose loop phi-joins an Int64 initializer
# with a Float64 update infers as `-> Union([I64, F64])`; consuming that call
# result in F64 arithmetic previously fell to a dynamic `+` call per iteration
# and degraded the consumer's accumulator slot to boxed `unknown`.  With the
# small-union fast path the consumer compiles to DynamicToF64 + AddF64 and the
# accumulator stays a typed F64 slot.

# Returns Union{Int64, Float64}: Int64 iff the loop body never runs.
function accum_mixed(n::Int)
    x = 0
    for i in 1:n
        x = x + 0.1
    end
    x
end

# Consumer: s::F64 + Union{I64,F64} — the Issue #9123 fast path fires here.
function consume_add(n::Int)
    s = 0.0
    for j in 1:n
        s = s + accum_mixed(3)
    end
    s
end

function consume_sub(n::Int)
    s = 100.0
    for j in 1:n
        s = s - accum_mixed(2)
    end
    s
end

function consume_mul(n::Int)
    s = 1.0
    for j in 1:n
        s = s * accum_mixed(4)
    end
    s
end

function consume_div(n::Int)
    s = 64.0
    for j in 1:n
        s = s / accum_mixed(1)
    end
    s
end

# The union side materializing as Int64 (empty loop): F64 + Int64 must still
# promote to Float64 with the exact same value as upstream Julia.
function consume_int_branch()
    s = 1.5
    s + accum_mixed(0)   # accum_mixed(0) == 0 (Int64)
end

# Issue #9145: `Union{Int64,Float64} <op> Int64` must union-split during
# inference so the result stays `Int64` when the union holds an `Int64` at
# runtime (empty loop), instead of the whole consumer wrongly inferring
# `Float64` and coercing the runtime `Int64` result to `Float64` at return.
# This must NOT go through the #9123 float-promotion fast path (only a
# concrete-F64 other operand qualifies). Julia: `union_plus_int(0) === 1`.
function union_plus_int(n::Int)
    accum_mixed(n) + 1
end

# Principle 10: the same union-left (and union-right) pattern for `-`, `*`, `==`.
union_minus_int(n::Int) = accum_mixed(n) - 1
union_times_int(n::Int) = accum_mixed(n) * 2
int_minus_union(n::Int) = 10 - accum_mixed(n)
union_eq_int(n::Int) = accum_mixed(n) == 0

# In-function loop-carried union (the original MWE shape): still upstream-exact.
function loop_phi_mwe()
    x = 0
    for i in 1:10
        x = x + 0.1
    end
    x
end

println(typeof(consume_add(5)) == Float64)
println(round(consume_add(5), digits=10) == 1.5)
println(round(consume_sub(4), digits=10) == 99.2)
println(round(consume_mul(3), digits=10) == round(0.4^3, digits=10))
println(consume_div(1) === 64.0 / 0.1 && typeof(consume_div(6)) == Float64)
println(consume_int_branch() === 1.5)
# Issue #9145 regression: standalone Union return preserves the Int64 tag
# (empty-loop path) and the Float64 tag otherwise.
println(accum_mixed(0) === 0)
println(accum_mixed(2) === 0.1 + 0.1)
# `Union{Int64,Float64} + Int64` — Int64+Int64=Int64, Float64+Int64=Float64.
println(union_plus_int(0) === 1)
println(union_plus_int(2) === 0.1 + 0.1 + 1)
# `-`, `*`, `==` with the union operand on either side.
println(union_minus_int(0) === -1)
println(union_minus_int(2) === (0.1 + 0.1) - 1)
println(union_times_int(0) === 0)
println(union_times_int(2) === (0.1 + 0.1) * 2)
println(int_minus_union(0) === 10)
println(int_minus_union(2) === 10 - (0.1 + 0.1))
println(union_eq_int(0) === true)
println(union_eq_int(2) === false)
println(typeof(loop_phi_mwe()) == Float64)
println(round(loop_phi_mwe(), digits=10) == 1.0)
"true\ntrue\ntrue\ntrue\ntrue\ntrue\ntrue\ntrue\ntrue\ntrue\ntrue\ntrue\ntrue\ntrue\ntrue\ntrue\ntrue\ntrue\ntrue\ntrue\n"
