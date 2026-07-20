# Issue #10566 (a): compound update x[i] *= c (IndexLoad + arithmetic +
# IndexStoreTyped) into a Vector{Int64}/{Float64} argument through an
# untyped function, exercising the runtime specializer's IndexAssign path.

function scale!(x, n, c)
    for i in 1:n
        x[i] *= c
    end
    return x
end

xi = scale!([1, 2, 3, 4, 5], 5, 2)
@assert xi == [2, 4, 6, 8, 10]

xf = scale!([1.0, 2.0, 3.0], 3, 0.5)
@assert xf == [0.5, 1.0, 1.5]

# Negative multiplier / accumulation across repeated calls.
y = [1, 1, 1]
scale!(y, 3, -1)
scale!(y, 3, -1)
@assert y == [1, 1, 1]

true
