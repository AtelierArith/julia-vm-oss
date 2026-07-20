# Issue #9524: state-dependent uncatchable abort in +(Rational{Int64}, Float32)
# after a mixed-type rem/mod/+ sweep.
#
# A fresh process returns 3.25f0 for `3//4 + Float32(2.5)`, but once the
# dispatch/specialization cache is populated by a preceding mixed-type sweep the
# Float32 operand was loose-matched to a `+(x::Rational, y::Integer)`-shaped
# method, entering the Rational constructor with an inconsistent field type and
# raising a VM-side `Inconsistent type inference for T` error that ESCAPED the
# surrounding try/catch and aborted the whole process (never reaching ALL DONE).
#
# Two guarantees are checked here:
#   1. Dispatch: +(Rational{Int64}, Float32) selects the promote fallback
#      regardless of prior dispatch-cache state, yielding 3.25f0 (Float32).
#   2. Robustness: any residual VM-side type-consistency error is a catchable
#      exception, so the full sweep completes and prints ALL DONE.

vals = Any[1, -1, typemax(Int64), UInt64(7),
           UInt128(3), Int128(9),
           3//4, -3//4, 1//1, typemax(Int64)//1, 2.5, Float32(2.5), true, Int8(5)]
ops = Any[(x, y) -> rem(x, y), (x, y) -> mod(x, y), (x, y) -> x + y]
for k in 1:length(ops)
    op = ops[k]
    for i in 1:length(vals), j in 1:length(vals)
        try
            op(vals[i], vals[j])
        catch e
        end
    end
end

# The sweep no longer aborts: this line is reached.
println("ALL DONE")

# After the cache is warm, the Rational/Float32 mixed ops still promote to the
# correct floating type and value (matching upstream julia's Number promotion).
r = 3//4 + Float32(2.5)
@assert r === 3.25f0
@assert (3//4 + 2.5) === 3.25
@assert (Float32(2.5) + 3//4) === 3.25f0
@assert (1//2 - Float32(0.25)) === 0.25f0
@assert (3//4 * Float32(2.0)) === 1.5f0
@assert (3//2 / Float32(3.0)) === 0.5f0

# Also verify through the same Any-typed dynamic dispatch used by the sweep.
a = vals[7]
b = vals[12]
s = a + b
@assert s === 3.25f0

println(r)
println(typeof(r))
true
