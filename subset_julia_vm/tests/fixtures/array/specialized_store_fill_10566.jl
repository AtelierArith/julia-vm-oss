# Issue #10566 (a): a fill loop that stores into a Vector{Int64}/{Float64}
# argument through an untyped function, exercising the runtime specializer's
# IndexAssign path (Issue #6346) now that a MemoryRef-backed Vector argument
# is recognized as ValueType::ArrayOf instead of ValueType::Struct.

function fill_const!(a, n, c)
    for i in 1:n
        a[i] = c
    end
    return a
end

function fill_const_f64!(a, n, c)
    for i in 1:n
        a[i] = c
    end
    return a
end

ai = fill_const!(zeros(Int64, 5), 5, 7)
@assert ai == [7, 7, 7, 7, 7]

af = fill_const_f64!(zeros(Float64, 4), 4, 2.5)
@assert af == [2.5, 2.5, 2.5, 2.5]

# Repeated calls (same specialized signature reused across calls).
b = zeros(Int64, 3)
for k in 1:3
    fill_const!(b, 3, k)
end
@assert b == [3, 3, 3]

# Empty range: loop body never runs.
c = [1, 2, 3]
fill_const!(c, 0, 99)
@assert c == [1, 2, 3]

true
