# Issue #10566 (a): map-copy y[i] = x[i] + 1 reading from one Vector{T}
# argument and storing into another, both MemoryRef-backed, through an
# untyped function -- exercises the runtime specializer's IndexAssign path
# (Issue #6346) for two distinct array arguments simultaneously.

function map_copy_plus1!(y, x, n)
    for i in 1:n
        y[i] = x[i] + 1
    end
    return y
end

xi = [10, 20, 30]
yi = zeros(Int64, 3)
map_copy_plus1!(yi, xi, 3)
@assert yi == [11, 21, 31]
@assert xi == [10, 20, 30]  # source untouched

xf = [1.5, 2.5]
yf = zeros(Float64, 2)
map_copy_plus1!(yf, xf, 2)
@assert yf == [2.5, 3.5]

true
