# VM-only benchmark for immutable-aggregate copy bandwidth (Issue #7966).
#
# Immutable structs and tuples are *deep-cloned* — their backing `Vec<Value>`
# is copied element-by-element — on every stack push, slot store, and
# function-call argument pass (no `Rc`/`Arc` sharing today). This driver builds
# small immutable `Vec3` structs and 3-tuples and shuffles them through
# function-call arguments, return values, and locals in a hot loop, the
# copy-heavy pattern that the `Rc`-sharing (Issue #7966 item 1) and
# `Value`-enum / `struct_name` shrink (item 2) work targets. Each component
# decays by 0.5 per iteration so the accumulator converges to a fixed value,
# keeping the result deterministic and independent of the trip count.

struct Vec3
    x::Float64
    y::Float64
    z::Float64
end

add(a::Vec3, b::Vec3) = Vec3(a.x + b.x, a.y + b.y, a.z + b.z)
scale(a::Vec3, s::Float64) = Vec3(a.x * s, a.y * s, a.z * s)
rotate(a::Vec3) = Vec3(a.y, a.z, a.x)

# Sum a 3-tuple's elements (exercises TupleValue deep-clone on the arg pass).
tsum(t) = t[1] + t[2] + t[3]

function walk(n)
    acc = Vec3(0.0, 0.0, 0.0)
    p = Vec3(1.0, -2.0, 3.0)
    for _ in 1:n
        q = scale(p, 0.5)          # copy p into arg, build + copy q
        acc = add(acc, q)          # copy acc, q into args, build + copy acc
        t = (q.x, q.y, q.z)        # build + copy a 3-tuple
        acc = scale(acc, 1.0 + tsum(t) * 0.0)  # tuple arg copy; identity scale
        p = scale(rotate(p), 0.5)  # rotate (copy) then shrink (copy)
    end
    return acc.x + acc.y + acc.z
end

function run_trials(trials, n)
    total = 0.0
    for _ in 1:trials
        total += walk(n)
    end
    return total
end

println(round(run_trials(200, 2000); digits=6))
